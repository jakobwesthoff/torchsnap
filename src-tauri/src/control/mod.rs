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
// rather than global events: typed, targeted at the launcher
// webview, and failing once that webview is gone.
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

use handler::{ControlError, HandlerRegistry};

use crate::settings::notifier::{SettingsNotifier, SettingsWatch};

// =========================================================
// ControlCommand — typed messages pushed to the frontend
// =========================================================

/// Messages sent from the control server to the frontend
/// through a Tauri `Channel`. The frontend's `useControlChannel`
/// hook dispatches these to the appropriate state setters.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ControlCommand {
    /// Reset the launcher UI state (query, selection, gadget view).
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
/// A send through a webview channel is a `webview.eval`, which fails
/// only when the webview is gone; the channel is then forgotten until
/// a new page subscribes. While a page reloads and before the new one
/// subscribes, a send succeeds and the command is lost.
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

    /// Whether a frontend has subscribed and its webview was not found
    /// gone by a send since. A page that is reloading still counts.
    pub fn is_subscribed(&self) -> bool {
        self.channel
            .lock()
            .expect("channel lock not poisoned")
            .is_some()
    }

    /// Hand a command to the frontend's webview.
    ///
    /// `Channel::send` runs the channel's callback on this thread, so
    /// the lock is not held while sending: a callback may call back into
    /// this state.
    fn send(&self, command: ControlCommand) -> Result<(), NotConnected> {
        let channel = self
            .channel
            .lock()
            .expect("channel lock not poisoned")
            .clone()
            .ok_or(NotConnected)?;
        if channel.send(command).is_ok() {
            return Ok(());
        }
        // The webview is gone. A page that subscribed in the meantime
        // keeps its channel.
        let mut stored = self.channel.lock().expect("channel lock not poisoned");
        if stored.as_ref().is_some_and(|c| c.id() == channel.id()) {
            *stored = None;
        }
        Err(NotConnected)
    }

    /// Hand a command to the frontend, as a control method's answer:
    /// `InvalidState` when no frontend could take it.
    pub fn deliver(&self, command: ControlCommand) -> Result<(), ControlError> {
        self.send(command)
            .map_err(|NotConnected| ControlError::InvalidState {
                message: NOT_CONNECTED.to_string(),
            })
    }
}

/// No frontend could take a command: none has subscribed, or its
/// webview is gone.
#[derive(Debug, PartialEq, Eq)]
pub struct NotConnected;

const NOT_CONNECTED: &str = "launcher frontend is not connected";

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
                                    handle_connection(stream, registry, app).await;
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

/// Longest request line the server reads, without its newline.
const MAX_REQUEST_BYTES: usize = 1024 * 1024;

/// One request line read from a client.
#[derive(Debug, PartialEq)]
enum RequestLine {
    Line(String),
    Eof,
    /// The client sent more than `MAX_REQUEST_BYTES` without a
    /// newline. The rest of the stream is not read.
    TooLong,
}

/// Read the next newline-terminated request, buffering at most
/// `MAX_REQUEST_BYTES` plus its newline. Invalid UTF-8 is an
/// `InvalidData` error.
async fn read_request_line<R>(reader: &mut R) -> std::io::Result<RequestLine>
where
    R: tokio::io::AsyncBufRead + Unpin,
{
    use tokio::io::AsyncReadExt as _;

    let mut bytes = Vec::new();
    let limit = MAX_REQUEST_BYTES as u64 + 1;
    let read = reader.take(limit).read_until(b'\n', &mut bytes).await?;
    if read == 0 {
        return Ok(RequestLine::Eof);
    }
    if bytes.len() > MAX_REQUEST_BYTES && bytes.last() != Some(&b'\n') {
        return Ok(RequestLine::TooLong);
    }
    String::from_utf8(bytes)
        .map(RequestLine::Line)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Handle a single client connection. Reads newline-delimited
/// JSON-RPC requests, dispatches to the handler registry, and
/// writes newline-delimited JSON-RPC responses.
async fn handle_connection(
    stream: tokio::net::UnixStream,
    registry: Arc<HandlerRegistry>,
    app: tauri::AppHandle,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = tokio::io::BufReader::new(reader);

    loop {
        match read_request_line(&mut reader).await {
            Ok(RequestLine::Eof) => break, // client disconnected
            Ok(RequestLine::TooLong) => {
                // The request has no readable id, so the error goes out
                // with a null id, and the connection closes since the
                // rest of the stream has no known request boundary.
                let response = json_rpc_error(
                    Value::Null,
                    -32600,
                    &format!("Invalid request: longer than {MAX_REQUEST_BYTES} bytes"),
                );
                let mut response_bytes =
                    serde_json::to_vec(&response).expect("serializing JSON-RPC response");
                response_bytes.push(b'\n');
                let _ = writer.write_all(&response_bytes).await;
                break;
            }
            Ok(RequestLine::Line(line)) => {
                // Handlers are synchronous and may block: on the main
                // thread (`on_main_thread`) or while waiting for an
                // answer. On the blocking pool such a wait holds none of
                // the runtime's workers, which async commands such as the
                // launcher's search run on.
                let registry = Arc::clone(&registry);
                let app = app.clone();
                let dispatched = tauri::async_runtime::spawn_blocking(move || {
                    process_request(&line, &registry, &app)
                })
                .await;
                // A handler that panicked ends the connection without a
                // response, as the panic did when it ran on this task.
                let Ok(response) = dispatched else {
                    break;
                };
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

#[cfg(test)]
pub(crate) mod test_channels {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tauri::ipc::{Channel, InvokeResponseBody};

    use super::ControlCommand;

    /// The JSON text of a message the backend sent through a channel.
    pub(crate) fn body_json(body: InvokeResponseBody) -> String {
        match body {
            InvokeResponseBody::Json(json) => json,
            InvokeResponseBody::Raw(_) => panic!("control commands are sent as JSON"),
        }
    }

    /// A frontend stand-in that records every command it receives.
    pub(crate) fn recording() -> (Channel<ControlCommand>, std::sync::Arc<Mutex<Vec<String>>>) {
        let received = std::sync::Arc::new(Mutex::new(Vec::new()));
        let sink = std::sync::Arc::clone(&received);
        let channel = Channel::new(move |body| {
            sink.lock().expect("test lock").push(body_json(body));
            Ok(())
        });
        (channel, received)
    }

    /// A channel whose webview is gone for the first send and answers
    /// every later one, so a test can tell whether a failed channel was
    /// kept.
    pub(crate) fn failing_once() -> Channel<ControlCommand> {
        let failed = AtomicBool::new(false);
        Channel::new(move |_| {
            if failed.swap(true, Ordering::SeqCst) {
                Ok(())
            } else {
                Err(tauri::Error::WebviewNotFound)
            }
        })
    }
}

#[cfg(test)]
mod channel_tests {
    use std::sync::Arc;

    use tauri::ipc::Channel;

    use super::test_channels::{failing_once, recording};
    use super::*;

    #[test]
    fn send_without_a_subscriber_is_not_connected() {
        let state = ControlChannelState::new();
        assert_eq!(state.send(ControlCommand::Dismiss), Err(NotConnected));
    }

    #[test]
    fn a_failing_send_is_not_connected_and_clears_the_channel() {
        let state = ControlChannelState::new();
        state.set(failing_once());
        assert_eq!(state.send(ControlCommand::Dismiss), Err(NotConnected));
        // The channel would answer this time; only a cleared one fails.
        assert_eq!(state.send(ControlCommand::Dismiss), Err(NotConnected));
    }

    #[test]
    fn a_channel_set_during_a_failing_send_survives() {
        let state = Arc::new(ControlChannelState::new());
        let (fresh, received) = recording();
        let resubscriber = Arc::clone(&state);
        let fresh = Mutex::new(Some(fresh));
        // A page that reloads while a command to the old one is on its
        // way: the new page subscribes before the old send fails.
        state.set(Channel::new(move |_| {
            if let Some(channel) = fresh.lock().expect("test lock").take() {
                resubscriber.set(channel);
            }
            Err(tauri::Error::WebviewNotFound)
        }));

        assert_eq!(state.send(ControlCommand::Dismiss), Err(NotConnected));
        assert_eq!(state.send(ControlCommand::Dismiss), Ok(()));
        assert_eq!(
            *received.lock().expect("test lock"),
            vec![r#"{"type":"dismiss"}"#.to_string()]
        );
    }

    #[test]
    fn send_through_a_channel_delivers_the_command() {
        let state = ControlChannelState::new();
        let (channel, received) = recording();
        state.set(channel);
        assert_eq!(
            state.send(ControlCommand::SetQuery {
                text: "firefox".into()
            }),
            Ok(())
        );
        assert_eq!(
            *received.lock().expect("test lock"),
            vec![r#"{"type":"setQuery","text":"firefox"}"#.to_string()]
        );
    }

    #[test]
    fn is_subscribed_follows_set_and_a_failed_send() {
        let state = ControlChannelState::new();
        assert!(!state.is_subscribed());
        state.set(failing_once());
        assert!(state.is_subscribed());
        let _ = state.send(ControlCommand::Dismiss);
        assert!(!state.is_subscribed());
    }

    #[test]
    fn deliver_maps_an_undelivered_command_to_invalid_state() {
        let state = ControlChannelState::new();
        let error = state
            .deliver(ControlCommand::Dismiss)
            .expect_err("no frontend is subscribed");
        assert_eq!(error.code(), -1);
        assert_eq!(error.message(), "launcher frontend is not connected");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn read_all(input: &[u8]) -> Vec<std::io::Result<RequestLine>> {
        let mut reader = tokio::io::BufReader::new(input);
        let mut lines = Vec::new();
        loop {
            let next = read_request_line(&mut reader).await;
            let done = !matches!(next, Ok(RequestLine::Line(_)));
            lines.push(next);
            if done {
                return lines;
            }
        }
    }

    #[tokio::test]
    async fn reads_newline_delimited_requests_until_eof() {
        let lines = read_all(b"{\"a\":1}\n{\"b\":2}").await;
        let lines: Vec<RequestLine> = lines.into_iter().map(|l| l.expect("read")).collect();
        assert_eq!(
            lines,
            vec![
                RequestLine::Line("{\"a\":1}\n".into()),
                RequestLine::Line("{\"b\":2}".into()),
                RequestLine::Eof,
            ]
        );
    }

    #[tokio::test]
    async fn accepts_a_request_of_exactly_the_limit() {
        let mut input = vec![b'x'; MAX_REQUEST_BYTES];
        input.push(b'\n');
        let lines = read_all(&input).await;
        assert!(matches!(&lines[0], Ok(RequestLine::Line(l)) if l.len() == MAX_REQUEST_BYTES + 1));
    }

    // A client that never sends a newline, or one huge line, must not
    // grow the host's buffer without bound.
    #[tokio::test]
    async fn stops_at_an_oversized_request() {
        let input = vec![b'x'; MAX_REQUEST_BYTES + 2];
        let lines = read_all(&input).await;
        assert!(
            matches!(lines[0], Ok(RequestLine::TooLong)),
            "{:?}",
            lines[0]
        );
        assert_eq!(lines.len(), 1);
    }

    // A client that streams bytes forever without a newline: the read
    // must end after the limit instead of buffering the whole stream.
    #[tokio::test]
    async fn stops_reading_an_endless_line_at_the_limit() {
        let mut reader = tokio::io::BufReader::new(tokio::io::repeat(b'x'));
        let line = read_request_line(&mut reader).await.expect("read");
        assert_eq!(line, RequestLine::TooLong);
    }

    #[tokio::test]
    async fn stops_at_an_oversized_line_followed_by_more() {
        let mut input = vec![b'x'; MAX_REQUEST_BYTES + 1];
        input.extend_from_slice(b"\n{}\n");
        let lines = read_all(&input).await;
        assert!(
            matches!(lines[0], Ok(RequestLine::TooLong)),
            "{:?}",
            lines[0]
        );
    }

    #[tokio::test]
    async fn rejects_invalid_utf8() {
        let lines = read_all(b"\xff\xfe\n").await;
        assert!(lines[0].is_err());
    }
}
