// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Control API — Unix Domain Socket JSON-RPC 2.0 Server
//
// Allows external processes to drive the launcher over a
// local Unix socket. The protocol is JSON-RPC 2.0 over
// newline-delimited JSON (one request per line, one response
// per line).
//
// The server is gated by the `controlChannel.enabled` setting
// and starts/stops reactively when the setting changes.
//
// Frontend communication uses a Tauri Channel<ControlCommand>
// rather than global events — typed, targeted, and lifecycle-
// aware (the channel is cleared if the webview is destroyed).
// =========================================================

pub mod handler;
mod handlers;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::Value;
use tauri::Manager;
use tauri::ipc::Channel;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
use tokio::sync::watch as tokio_watch;

use handler::HandlerRegistry;

use crate::settings_notifier::{SettingsNotifier, SettingsWatch};

// =========================================================
// ControlCommand — typed messages pushed to the frontend
// =========================================================

/// Messages sent from the control server to the frontend
/// through a Tauri `Channel`. The frontend's `useControlChannel`
/// hook dispatches these to the appropriate state setters.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ControlCommand {
    /// Reset the launcher UI state (query, selection, plugin view).
    Dismiss,
    /// Set the search input text.
    SetQuery { text: String },
}

// =========================================================
// ControlChannelState — shared channel to the frontend
// =========================================================

/// Holds the Tauri channel to the frontend, set by the
/// `control_subscribe` command. Handlers push `ControlCommand`
/// messages through this channel.
///
/// Gracefully handles a closed channel (e.g., the webview was
/// destroyed for memory optimization) by clearing the stale
/// reference. The frontend re-subscribes when recreated.
pub struct ControlChannelState {
    channel: Mutex<Option<Channel<ControlCommand>>>,
}

impl ControlChannelState {
    pub fn new() -> Self {
        Self {
            channel: Mutex::new(None),
        }
    }

    /// Store a new channel from a `control_subscribe` invocation.
    pub fn set(&self, channel: Channel<ControlCommand>) {
        *self.channel.lock().expect("channel lock not poisoned") = Some(channel);
    }

    /// Send a command to the frontend. If the channel is closed
    /// (webview destroyed), the stale reference is cleared and the
    /// send is silently skipped.
    pub fn send(&self, command: ControlCommand) {
        let mut guard = self.channel.lock().expect("channel lock not poisoned");
        if let Some(ch) = guard.as_ref()
            && ch.send(command).is_err()
        {
            // Channel closed — webview was destroyed. Clear the
            // stale reference; the frontend will re-subscribe when
            // the webview is recreated.
            *guard = None;
        }
    }
}

// =========================================================
// ControlServer
// =========================================================

struct ControlServer {
    registry: Arc<HandlerRegistry>,
    shutdown_tx: Option<tokio_watch::Sender<bool>>,
    socket_path: Option<PathBuf>,
}

impl ControlServer {
    fn new(registry: Arc<HandlerRegistry>) -> Self {
        Self {
            registry,
            shutdown_tx: None,
            socket_path: None,
        }
    }

    fn start(&mut self, app: &tauri::AppHandle) {
        let path = socket_path(app);

        // Remove any stale socket file from a previous run.
        let _ = std::fs::remove_file(&path);

        let listener = match tokio::net::UnixListener::bind(&path) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("control: failed to bind {}: {e}", path.display());
                return;
            }
        };

        let (shutdown_tx, mut shutdown_rx) = tokio_watch::channel(false);
        self.shutdown_tx = Some(shutdown_tx);
        self.socket_path = Some(path.clone());

        let registry = Arc::clone(&self.registry);
        let app_handle = app.clone();

        tauri::async_runtime::spawn(async move {
            loop {
                tokio::select! {
                    result = listener.accept() => {
                        match result {
                            Ok((stream, _addr)) => {
                                let registry = Arc::clone(&registry);
                                let app = app_handle.clone();
                                tokio::spawn(async move {
                                    handle_connection(stream, &registry, &app).await;
                                });
                            }
                            Err(e) => {
                                eprintln!("control: accept error: {e}");
                            }
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        break;
                    }
                }
            }

            // Clean up socket file after the accept loop exits.
            let _ = std::fs::remove_file(&path);
        });
    }

    fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(true);
        }
        if let Some(path) = self.socket_path.take() {
            let _ = std::fs::remove_file(&path);
        }
    }
}

// =========================================================
// Connection handling
// =========================================================

/// Handle a single client connection. Reads newline-delimited
/// JSON-RPC requests, dispatches to the handler registry, and
/// writes newline-delimited JSON-RPC responses.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    registry: &HandlerRegistry,
    app: &tauri::AppHandle,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => break, // EOF — client disconnected
            Ok(_) => {
                let response = process_request(&line, registry, app);
                let mut response_bytes =
                    serde_json::to_vec(&response).expect("serializing JSON-RPC response");
                response_bytes.push(b'\n');
                if writer.write_all(&response_bytes).await.is_err() {
                    break;
                }
            }
            Err(_) => break,
        }
    }
}

// =========================================================
// JSON-RPC 2.0 dispatch
// =========================================================

/// Parse a JSON-RPC 2.0 request, dispatch to the matching
/// handler, and build the response envelope.
fn process_request(line: &str, registry: &HandlerRegistry, app: &tauri::AppHandle) -> Value {
    // 1. Parse JSON
    let parsed: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return json_rpc_error(Value::Null, -32700, "Parse error"),
    };

    // 2. Validate JSON-RPC envelope
    let id = parsed.get("id").cloned().unwrap_or(Value::Null);
    let method = match parsed.get("method").and_then(|m| m.as_str()) {
        Some(m) => m,
        None => return json_rpc_error(id, -32600, "Invalid request: missing 'method'"),
    };

    // 3. Extract params (default to null if omitted)
    let params = parsed.get("params").cloned().unwrap_or(Value::Null);

    // 4. Look up handler
    let handler = match registry.get(method) {
        Some(h) => h,
        None => return json_rpc_error(id, -32601, &format!("Method not found: {method}")),
    };

    // 5. Call handler and build response
    match handler.handle(params, app) {
        Ok(result) => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": result,
        }),
        Err(e) => serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": e.to_json_rpc_error(),
        }),
    }
}

fn json_rpc_error(id: Value, code: i64, message: &str) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

// =========================================================
// Socket path
// =========================================================

fn socket_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_data_dir()
        .expect("resolve app data dir")
        .join("control.sock")
}

// =========================================================
// Lifecycle reactor
//
// Watches the `controlChannel.enabled` setting and starts or
// stops the Unix socket server accordingly. Runs as an async
// task for the lifetime of the app.
// =========================================================

pub fn start_control_server_reactor(
    app: &tauri::AppHandle,
    notifier: &Arc<SettingsNotifier>,
    store: &Arc<tauri_plugin_store::Store<tauri::Wry>>,
) {
    let initial_enabled: bool = store
        .get("controlChannel.enabled")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or(false);

    let mut enabled_watch: SettingsWatch<bool> =
        notifier.watch_with_initial("controlChannel.enabled", Value::from(initial_enabled));

    let app_handle = app.clone();

    tauri::async_runtime::spawn(async move {
        // Build the handler registry once and share it across
        // server restarts.
        let mut registry = HandlerRegistry::new();
        handlers::register_all(&mut registry);
        let registry = Arc::new(registry);

        let mut server: Option<ControlServer> = None;

        // Start immediately if the setting is already enabled.
        if initial_enabled {
            let mut s = ControlServer::new(Arc::clone(&registry));
            s.start(&app_handle);
            server = Some(s);
        }

        // React to setting changes for the lifetime of the app.
        loop {
            let Some(new_enabled) = enabled_watch.changed().await else {
                break; // Notifier dropped — app shutting down.
            };

            if new_enabled && server.is_none() {
                let mut s = ControlServer::new(Arc::clone(&registry));
                s.start(&app_handle);
                server = Some(s);
            } else if !new_enabled && let Some(mut s) = server.take() {
                s.stop();
            }
        }

        // Final cleanup on exit.
        if let Some(mut s) = server.take() {
            s.stop();
        }
    });
}
