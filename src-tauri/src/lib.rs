// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod coalescing_dispatcher;
mod control;
mod frecency;
mod icons;
mod network;
mod platform;
mod plugin_host;
mod plugins;
mod search;
mod settings;
mod settings_notifier;
mod storage;
mod unicode;
mod wasm;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::Context;
use tauri::{
    Listener, Manager, RunEvent, WebviewUrl, WindowEvent, ipc::Channel,
    webview::WebviewWindowBuilder,
};

#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;

use platform::{LauncherPanel as _, PlatformLauncherPanel, PlatformTray, Tray as _};

// =========================================================
// Launcher Window Layout
//
// The launcher window is sized to tightly fit its content
// rather than filling the entire screen. This keeps the
// WebKit backing-store allocation proportional to the actual
// UI area instead of the full monitor resolution (ADR 0020).
//
// Layout dimensions are defined on the frontend side
// (`src/launcher/layout.ts`) — the single source of truth —
// and sent to the backend via the `launcher_set_layout`
// command after React mounts. The first show is gated on
// this signal to prevent flashing an unsized window.
// =========================================================

/// Window layout dimensions received from the frontend.
///
/// Set once at mount time via `launcher_set_layout` and never
/// changes for the lifetime of the app. The `OnceLock` doubles
/// as a readiness gate: `toggle_launcher_window` will not show
/// the panel until the layout has been received.
#[derive(Debug, Clone, Copy)]
struct LauncherLayout {
    window_width: f64,
    window_height: f64,
    card_top_offset: f64,
}

/// Managed state wrapper. The `OnceLock` is empty until the
/// frontend sends layout dimensions, at which point it is set
/// exactly once. If a show was requested before the layout
/// arrived, `show_pending` is set so that `launcher_set_layout`
/// can trigger the show once the dimensions are available.
struct LauncherLayoutState {
    layout: OnceLock<LauncherLayout>,
    show_pending: AtomicBool,
}

impl LauncherLayoutState {
    fn new() -> Self {
        Self {
            layout: OnceLock::new(),
            show_pending: AtomicBool::new(false),
        }
    }

    fn set(&self, layout: LauncherLayout) {
        // Ignore if already set (e.g. hot-reload sending it twice).
        let _ = self.layout.set(layout);
    }

    fn get(&self) -> Option<&LauncherLayout> {
        self.layout.get()
    }

    fn request_show(&self) {
        self.show_pending.store(true, Ordering::Relaxed);
    }

    fn take_pending_show(&self) -> bool {
        self.show_pending.swap(false, Ordering::Relaxed)
    }
}

// =========================================================
// Auxiliary Windows (Settings, Developer Tools)
//
// Both are created on demand, destroyed when closed, and
// use the same present/center/focus logic. The shared
// `present_auxiliary_window` handles macOS-specific behavior
// (MoveToActiveSpace) and cursor-relative centering.
// =========================================================

/// Position, show, and focus an auxiliary window on the
/// monitor under the cursor.
fn present_auxiliary_window(win: &tauri::WebviewWindow, app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

        let ns_window = win.ns_window().expect("auxiliary NSWindow handle");
        // SAFETY: Tauri's `ns_window()` returns a valid `*mut c_void`
        // pointing to the underlying NSWindow. The pointer is valid for
        // the lifetime of the WebviewWindow and we only borrow it
        // briefly to set a collection behavior flag.
        let ns_window: &NSWindow = unsafe { &*(ns_window as *const NSWindow) };
        ns_window.setCollectionBehavior(NSWindowCollectionBehavior::MoveToActiveSpace);
    }

    if let Some(monitor) = monitor_under_cursor(app)
        && let Ok(win_size) = win.outer_size()
    {
        let mon_size = monitor.size();
        let mon_pos = monitor.position();
        let scale = monitor.scale_factor();

        let x = mon_pos.x as f64 / scale
            + (mon_size.width as f64 / scale - win_size.width as f64 / scale) / 2.0;
        let y = mon_pos.y as f64 / scale
            + (mon_size.height as f64 / scale - win_size.height as f64 / scale) / 2.0;

        let _ = win.set_position(tauri::LogicalPosition::new(x, y));
    }

    let _ = win.show();
    let _ = win.set_focus();
}

/// Configuration for an on-demand auxiliary window.
struct AuxiliaryWindowConfig {
    label: &'static str,
    url: &'static str,
    title: &'static str,
    width: f64,
    height: f64,
    min_width: f64,
    min_height: f64,
}

/// Build an on-demand auxiliary window that stays hidden until the
/// frontend emits `"react-ready"`. If the window already exists it
/// is simply brought to front.
fn show_auxiliary_window(app: &tauri::AppHandle, config: &AuxiliaryWindowConfig) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        let _ = app.show();
    }

    // If the window is already alive just bring it to front.
    if let Some(existing) = app.get_webview_window(config.label) {
        present_auxiliary_window(&existing, app);
        return;
    }

    // Build the window hidden — the frontend will signal readiness.
    let mut builder =
        WebviewWindowBuilder::new(app, config.label, WebviewUrl::App(config.url.into()))
            .title(config.title)
            .inner_size(config.width, config.height)
            .min_inner_size(config.min_width, config.min_height)
            .resizable(true)
            .visible(false)
            .focused(false)
            .center();

    #[cfg(target_os = "macos")]
    {
        builder = builder
            .title_bar_style(TitleBarStyle::Overlay)
            .hidden_title(true);
    }

    let win = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("failed to create {} window: {e:#}", config.label);
            return;
        }
    };

    let handle = app.clone();
    let window_label = config.label.to_string();
    win.once("react-ready", move |_| {
        let inner_handle = handle.clone();
        let label = window_label;
        let _ = handle.run_on_main_thread(move || {
            if let Some(win) = inner_handle.get_webview_window(&label) {
                present_auxiliary_window(&win, &inner_handle);
            }
        });
    });
}

// =========================================================
// Settings Window
// =========================================================

const SETTINGS_WINDOW: AuxiliaryWindowConfig = AuxiliaryWindowConfig {
    label: "settings",
    url: "settings.html",
    title: "Torchsnap Settings",
    width: 720.0,
    height: 520.0,
    min_width: 600.0,
    min_height: 400.0,
};

pub(crate) fn show_settings_window(app: &tauri::AppHandle) {
    show_auxiliary_window(app, &SETTINGS_WINDOW);
}

// =========================================================
// Developer Tools Window
// =========================================================

const DEVTOOLS_WINDOW: AuxiliaryWindowConfig = AuxiliaryWindowConfig {
    label: "devtools",
    url: "devtools.html",
    title: "Torchsnap Developer Tools",
    width: 900.0,
    height: 600.0,
    min_width: 700.0,
    min_height: 400.0,
};

pub(crate) fn show_devtools_window(app: &tauri::AppHandle) {
    show_auxiliary_window(app, &DEVTOOLS_WINDOW);
}

// =========================================================
// Launcher Window
// =========================================================

fn monitor_under_cursor(app: &tauri::AppHandle) -> Option<tauri::Monitor> {
    let cursor = app.cursor_position().ok()?;

    app.available_monitors()
        .ok()
        .and_then(|monitors| {
            monitors.into_iter().find(|m| {
                let pos = m.position();
                let size = m.size();
                let x = pos.x as f64;
                let y = pos.y as f64;
                let w = size.width as f64;
                let h = size.height as f64;
                cursor.x >= x && cursor.x < x + w && cursor.y >= y && cursor.y < y + h
            })
        })
        .or_else(|| app.primary_monitor().ok().flatten())
}

pub(crate) fn position_launcher_on_cursor_monitor(app: &tauri::AppHandle, layout: &LauncherLayout) {
    if let Some(monitor) = monitor_under_cursor(app) {
        let size = monitor.size();
        let pos = monitor.position();
        let scale = monitor.scale_factor();

        let monitor_w = size.width as f64 / scale;
        let monitor_h = size.height as f64 / scale;
        let monitor_x = pos.x as f64 / scale;
        let monitor_y = pos.y as f64 / scale;

        // Center the launcher window horizontally on the monitor.
        // Vertically, place the card at ~25% of monitor height by
        // offsetting the window top so that the card (which sits at
        // card_top_offset within the window) lands there.
        let win_x = monitor_x + (monitor_w - layout.window_width) / 2.0;
        let win_y = monitor_y + (0.25 * monitor_h) - layout.card_top_offset;

        if let Err(e) = PlatformLauncherPanel::set_frame(
            app,
            win_x,
            win_y,
            layout.window_width,
            layout.window_height,
        ) {
            eprintln!("failed to set launcher frame: {e:#}");
        }
    }
}

/// Shrink the launcher window to a tiny size so that WebKit can
/// release its full-size backing stores while the panel is hidden.
fn shrink_launcher_window(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let _ = win.set_size(tauri::LogicalSize::new(1.0, 1.0));
}

/// Hide the launcher panel and shrink the window to reclaim
/// WebKit backing-store memory.
pub(crate) fn hide_launcher(app: &tauri::AppHandle) {
    if let Err(e) = PlatformLauncherPanel::hide(app) {
        eprintln!("failed to hide launcher: {e:#}");
    }
    shrink_launcher_window(app);
}

/// Tauri command so the frontend can hide the launcher through
/// the same path as the hotkey toggle and control API, ensuring
/// the window shrink always happens.
#[tauri::command]
fn launcher_hide(app: tauri::AppHandle) {
    hide_launcher(&app);
}

/// Tauri command called by the frontend after React mounts to
/// report the launcher's layout dimensions. This also serves as
/// the readiness signal — the first show is gated on it.
#[tauri::command]
fn launcher_set_layout(
    window_width: f64,
    window_height: f64,
    card_top_offset: f64,
    app: tauri::AppHandle,
) {
    let layout = LauncherLayout {
        window_width,
        window_height,
        card_top_offset,
    };
    let state = app.state::<LauncherLayoutState>();
    state.set(layout);

    // Set the frame and warm up the compositor so that the first
    // real show has no flash of empty content.
    position_launcher_on_cursor_monitor(&app, &layout);
    if let Err(e) = PlatformLauncherPanel::warm_up(&app) {
        eprintln!("failed to warm up launcher: {e:#}");
    }

    // If a show was requested before the layout arrived, trigger
    // it now that the window is ready.
    if state.take_pending_show() {
        position_launcher_on_cursor_monitor(&app, &layout);
        if let Err(e) = PlatformLauncherPanel::show(&app) {
            eprintln!("failed to show launcher (deferred): {e:#}");
        }
    }
}

pub(crate) fn toggle_launcher_window(app: &tauri::AppHandle) {
    let is_visible = PlatformLauncherPanel::is_visible(app).unwrap_or(false);

    if is_visible {
        hide_launcher(app);
        return;
    }

    // If the frontend hasn't reported layout dimensions yet,
    // queue the show so it fires once the layout arrives.
    let layout_state = app.state::<LauncherLayoutState>();
    let Some(layout) = layout_state.get().copied() else {
        layout_state.request_show();
        return;
    };

    position_launcher_on_cursor_monitor(app, &layout);

    if let Err(e) = PlatformLauncherPanel::show(app) {
        eprintln!("failed to show launcher: {e:#}");
    }
}

// =========================================================
// Frecency Commands
// =========================================================

#[tauri::command]
fn frecency_stats(
    frecency: tauri::State<'_, Arc<frecency::FrecencyStore>>,
) -> Result<frecency::FrecencyStats, String> {
    frecency.stats().map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn frecency_clear(frecency: tauri::State<'_, Arc<frecency::FrecencyStore>>) -> Result<(), String> {
    frecency.clear_all().map_err(|e| format!("{e:#}"))
}

#[tauri::command]
fn website_metadata_stats(
    service: tauri::State<'_, Arc<network::website_metadata::WebsiteMetadataService>>,
) -> Result<serde_json::Value, String> {
    let stats = service.stats();
    Ok(serde_json::json!({
        "entryCount": stats.entry_count,
        "faviconBytes": stats.favicon_bytes,
    }))
}

#[tauri::command]
fn website_metadata_clear_cache(
    service: tauri::State<'_, Arc<network::website_metadata::WebsiteMetadataService>>,
) -> Result<(), String> {
    service.clear_cache().map_err(|e| format!("{e:#}"))
}

// =========================================================
// WASM Plugin Manifests
// =========================================================

/// Returns the full manifest for every loaded WASM plugin.
/// Called once per webview at startup to register dynamic
/// plugin components and settings entries.
#[tauri::command]
fn wasm_plugins(
    registry: tauri::State<'_, wasm::protocol::PluginSourceRegistry>,
) -> Vec<wasm::manifest::Manifest> {
    let sources = registry.read().expect("source registry not poisoned");
    sources.values().map(|s| s.manifest().clone()).collect()
}

// =========================================================
// Control API — frontend channel subscription
// =========================================================

/// Called by the frontend at mount time to establish the
/// control channel. The channel is stored in managed state
/// so that control handlers can push commands to it.
#[tauri::command]
fn control_subscribe(channel: Channel<control::ControlCommand>, app: tauri::AppHandle) {
    app.state::<control::ControlChannelState>().set(channel);
}

// =========================================================
// App Entry Point
// =========================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        // The command registry must stay in sync with the frontend's
        // typed `command()` wrapper in `src/lib/command.ts`. When
        // adding, removing, or changing a command signature here,
        // update the `CommandMap` interface on the frontend as well.
        .invoke_handler(tauri::generate_handler![
            search::search,
            search::search_execute,
            search::plugin_message,
            control_subscribe,
            launcher_hide,
            launcher_set_layout,
            frecency_stats,
            frecency_clear,
            website_metadata_stats,
            website_metadata_clear_cache,
            wasm::logging::commands::devtools_log_history,
            wasm::logging::commands::devtools_log_subscribe,
            wasm::logging::commands::devtools_log_clear,
            wasm::logging::commands::devtools_log_stats,
            wasm::logging::commands::logger_emit,
            wasm::logging::commands::logger_span_start,
            wasm::logging::commands::logger_span_end,
            wasm_plugins,
        ])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ));

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    // Shared registry mapping plugin IDs to their sources.
    // The protocol handler reads from these to serve frontend
    // assets; plugin loading populates the registry.
    let plugin_source_registry = wasm::protocol::new_registry();
    let builder = wasm::protocol::register_plugin_protocol(
        builder,
        Arc::clone(&plugin_source_registry),
    );

    let app = builder
        .setup(move |app| {
            // =========================================================
            // Global settings defaults
            // =========================================================
            use tauri_plugin_store::StoreExt;

            let store = app.store("settings.json").expect("settings store");
            settings::SettingsInit::from_store(&store, "")
                .ensure("globalShortcut", "CmdOrCtrl+Shift+Space")
                .ensure("mascotMode", "center")
                .ensure("randomMascots", true)
                .ensure("showNsfwMascots", true)
                .ensure("controlChannel.enabled", false)
                .ensure("frecency.enabled", true)
                .ensure("websiteMetadata.cacheTtlDays", 30)
                .apply(&store, "");

            // =========================================================
            // Settings notifier
            // =========================================================
            let notifier = Arc::new(settings_notifier::SettingsNotifier::new());

            // =========================================================
            // Frecency store
            // =========================================================
            let app_data_dir = app.path().app_data_dir().context("resolve app data dir")?;
            let frecency_store = frecency::FrecencyStore::open(&app_data_dir, &notifier, &store)
                .context("initialize frecency store")?;
            let frecency_store = Arc::new(frecency_store);

            // =========================================================
            // Plugin host
            //
            // Central authority for plugin lifecycle: registration,
            // settings init, parallel enable, shortcut management,
            // search routing, and shutdown.
            // =========================================================
            let mut host = plugin_host::PluginHost::new(
                Arc::clone(&store),
                Arc::clone(&frecency_store),
            );
            host.register(Box::new(plugins::commands::BuiltInCommandsPlugin));
            host.register(Box::new(
                plugins::system_commands::SystemCommandsPlugin::new(),
            ));

            let icon_cache_dir = app
                .path()
                .app_cache_dir()
                .context("resolve app cache dir")?
                .join("icons");
            let icon_cache = Arc::new(icons::IconCache::new(icon_cache_dir));
            host.register(Box::new(plugins::app_launcher::AppLauncherPlugin::new(
                platform::PlatformAppDiscovery,
                Arc::clone(&icon_cache),
            )));
            host.register(Box::new(
                plugins::system_preferences::SystemPreferencesPlugin::new(
                    platform::PlatformSettingsDiscovery,
                    Arc::clone(&icon_cache),
                ),
            ));
            host.register(Box::new(plugins::clipboard::ClipboardPlugin::new(
                platform::PlatformClipboard,
            )));
            host.register(Box::new(plugins::emoji::EmojiPickerPlugin::new()));
            host.register(Box::new(plugins::calculator::CalculatorPlugin::new()));

            // =========================================================
            // Website metadata service
            //
            // Shared service for fetching and caching website metadata
            // (title, description, favicon). Plugins that need domain
            // favicons receive an Arc to this service at construction.
            // =========================================================
            let metadata_cache_dir = app
                .path()
                .app_cache_dir()
                .context("resolve app cache dir")?
                .join("website-metadata");
            let initial_ttl: u32 = store
                .get("websiteMetadata.cacheTtlDays")
                .and_then(|v| serde_json::from_value(v).ok())
                .unwrap_or(30);
            let metadata_service = Arc::new(
                network::website_metadata::WebsiteMetadataService::new(
                    metadata_cache_dir,
                    &notifier,
                    initial_ttl,
                )
                .context("initialize website metadata service")?,
            );
            metadata_service.start_retention();

            host.register(Box::new(plugins::open_url::OpenUrlPlugin::new(
                Arc::clone(&metadata_service),
            )));
            host.register(Box::new(plugins::bangs::BangsPlugin::new(
                Arc::clone(&metadata_service),
            )));

            // =========================================================
            // Logging system
            //
            // Structured logging for WASM plugins. Started before
            // plugin loading so that compilation and instantiation
            // timing is captured from the very first plugin.
            // =========================================================
            let logging_system = wasm::logging::channel::LoggingSystem::start();
            let log_sender = logging_system.sender();
            let span_registry = Arc::new(wasm::logging::spans::SpanRegistry::new());
            let logging_system = Arc::new(logging_system);

            // =========================================================
            // WASM plugins
            //
            // Load plugins from the `plugins/` development directory.
            // Each subdirectory with a manifest.toml is loaded as a
            // DirectorySource, instantiated via wasmtime, and bridged
            // to the native Plugin trait.
            //
            // TODO: Replace hardcoded dev path with proper plugin
            // discovery from $APPDATA/torchsnap/plugins/.
            // =========================================================
            match load_wasm_plugins(&mut host, &log_sender, &span_registry, &plugin_source_registry) {
                Ok(count) => {
                    if count > 0 {
                        log_sender.send(wasm::logging::LogItem {
                            seq: 0,
                            timestamp: std::time::SystemTime::now(),
                            source: wasm::logging::LogSource::Host,
                            kind: wasm::logging::LogItemKind::Message {
                                level: wasm::logging::LogLevel::Info,
                                message: format!("Loaded {count} WASM plugin(s)"),
                                metadata: vec![],
                                span_id: None,
                            },
                        });
                    }
                }
                Err(e) => {
                    log_sender.send(wasm::logging::LogItem {
                        seq: 0,
                        timestamp: std::time::SystemTime::now(),
                        source: wasm::logging::LogSource::Host,
                        kind: wasm::logging::LogItemKind::Message {
                            level: wasm::logging::LogLevel::Error,
                            message: format!("Failed to initialize WASM plugins: {e:#}"),
                            metadata: vec![],
                            span_id: None,
                        },
                    });
                }
            }

            // Settings init + shortcut registration + parallel setup.
            host.initialize_and_start(app.handle());

            let host = Arc::new(host);

            // Spawn the shortcut reactor — watches for settings changes
            // and re-registers all shortcuts when relevant keys change.
            host.start_shortcut_reactor(app.handle());

            app.manage(Arc::clone(&host));
            app.manage(Arc::clone(&frecency_store));
            app.manage(Arc::clone(&metadata_service));
            app.manage(Arc::clone(&logging_system));
            app.manage(Arc::clone(&span_registry));
            app.manage(plugin_source_registry);

            // =========================================================
            // Settings-changed listener
            //
            // Propagates store changes to:
            // 1. Watch channels (SettingsNotifier) for non-plugin
            //    SettingsWatch subscribers (FrecencyStore, control, etc.)
            // 2. Host-managed plugin lifecycle (enable/disable) and
            //    settings dispatch (setting_changed) via
            //    CoalescingDispatcher
            // 3. Shortcut reactor for re-registration
            // =========================================================
            {
                let notifier = Arc::clone(&notifier);
                let store_for_listener = Arc::clone(&store);
                let host_for_listener = Arc::clone(&host);
                let app_for_listener = app.handle().clone();
                app.listen("settings-changed", move |event: tauri::Event| {
                    #[derive(serde::Deserialize)]
                    struct Payload {
                        key: String,
                    }
                    if let Ok(payload) = serde_json::from_str::<Payload>(event.payload()) {
                        let value = store_for_listener
                            .get(&payload.key)
                            .unwrap_or(serde_json::Value::Null);

                        // Legacy: propagate to watch channels.
                        notifier.notify(&payload.key, value.clone());

                        // New: route to host-managed lifecycle and
                        // plugin setting_changed dispatch.
                        host_for_listener.handle_setting_changed(
                            &payload.key,
                            value,
                            &app_for_listener,
                        );

                        // Signal shortcut re-registration if the changed
                        // key affects shortcuts or plugin enabled state.
                        if host_for_listener.is_key_watched(&payload.key) {
                            host_for_listener.notify_shortcut_change();
                        }
                    }
                });
            }

            // =========================================================
            // Control API server (Unix domain socket, JSON-RPC 2.0)
            // =========================================================
            app.manage(control::ControlChannelState::new());
            app.manage(LauncherLayoutState::new());
            control::start_control_server_reactor(app.handle(), &notifier, &store);

            // =========================================================
            // Hide dock icon (macOS only)
            // =========================================================
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // =========================================================
            // Tray icon with context menu
            // =========================================================
            PlatformTray::build(app, toggle_launcher_window, show_settings_window, show_devtools_window)
                .context("build platform tray")?;

            // =========================================================
            // Preload windows
            // =========================================================

            let launcher_win =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("launcher.html".into()))
                    .transparent(true)
                    .decorations(false)
                    .shadow(false)
                    .visible(false)
                    .focused(false)
                    .title("")
                    .build()
                    .context("create launcher window")?;

            PlatformLauncherPanel::init(&launcher_win)
                .context("initialize platform launcher panel")?;

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Intercept window close for the launcher: hide instead of destroying,
    // so the menubar app keeps running. The settings window is allowed to
    // close normally — it will be recreated on demand next time the user
    // opens it, keeping RAM usage low while it is not visible.
    // On exit, disable all plugins.
    app.run(|app, event| match &event {
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == "main" => {
            api.prevent_close();
            if let Some(win) = app.get_webview_window(label) {
                let _ = win.hide();
            }
        }
        RunEvent::Exit => {
            let metadata = app.state::<Arc<network::website_metadata::WebsiteMetadataService>>();
            metadata.teardown();

            let host = app.state::<Arc<plugin_host::PluginHost>>();
            host.disable_all();

            // Belt-and-suspenders cleanup for the control socket.
            // The reactor task also cleans up, but this is synchronous
            // and guaranteed to run.
            if let Ok(data_dir) = app.path().app_data_dir() {
                let _ = std::fs::remove_file(data_dir.join("control.sock"));
            }
        }
        _ => {}
    });
}

// =========================================================
// WASM Plugin Loader
//
// Scans the `plugins/` development directory for WASM plugin
// directories, instantiates each one, and registers them
// with the PluginHost.
//
// TODO: Replace hardcoded dev path with proper plugin
// discovery from $APPDATA/torchsnap/plugins/ and support
// for .torchsnap archive files via ArchiveSource.
// =========================================================

fn load_wasm_plugins(
    host: &mut plugin_host::PluginHost,
    log_sender: &wasm::logging::channel::LogSender,
    span_registry: &Arc<wasm::logging::spans::SpanRegistry>,
    source_registry: &wasm::protocol::PluginSourceRegistry,
) -> anyhow::Result<usize> {
    let runtime = wasm::runtime::WasmRuntime::new(
        log_sender.clone(),
        Arc::clone(span_registry),
    )?;

    let plugin_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins");
    let entries = match std::fs::read_dir(&plugin_dir) {
        Ok(entries) => entries,
        // No plugins directory — not an error, just nothing to load.
        Err(_) => return Ok(0),
    };

    let mut count = 0;

    for entry in entries.flatten() {
        let path = entry.path();

        // Load .torchsnap archives or plugin directories.
        // When both exist (e.g., hello-world/ alongside
        // hello-world.torchsnap), the archive takes precedence
        // and the directory is skipped.
        let is_archive = path.extension().is_some_and(|ext| ext == "torchsnap");
        let is_directory = path.is_dir() && path.join("manifest.toml").exists();

        if !is_archive && !is_directory {
            continue;
        }

        if is_directory {
            // Check if a .torchsnap archive exists alongside
            // the directory. If so, skip the directory — the
            // archive will be loaded when the iterator reaches it.
            let archive_path = path.with_extension("torchsnap");
            if archive_path.exists() {
                continue;
            }
        }

        let source_kind = if is_archive { "archive" } else { "directory" };
        let loaded = load_single_wasm_plugin(&runtime, &path, host, log_sender, source_registry);
        match loaded {
            Ok(plugin_id) => {
                log_sender.send(wasm::logging::LogItem {
                    seq: 0,
                    timestamp: std::time::SystemTime::now(),
                    source: wasm::logging::LogSource::Host,
                    kind: wasm::logging::LogItemKind::Message {
                        level: wasm::logging::LogLevel::Info,
                        message: format!("Loaded plugin: {plugin_id} ({source_kind})"),
                        metadata: vec![
                            ("plugin_id".to_string(), plugin_id),
                            ("source".to_string(), source_kind.to_string()),
                        ],
                        span_id: None,
                    },
                });
                count += 1;
            }
            Err(e) => {
                log_sender.send(wasm::logging::LogItem {
                    seq: 0,
                    timestamp: std::time::SystemTime::now(),
                    source: wasm::logging::LogSource::Host,
                    kind: wasm::logging::LogItemKind::Message {
                        level: wasm::logging::LogLevel::Error,
                        message: format!("Failed to load {}: {e:#}", path.display()),
                        metadata: vec![],
                        span_id: None,
                    },
                });
            }
        }
    }

    Ok(count)
}

fn load_single_wasm_plugin(
    runtime: &wasm::runtime::WasmRuntime,
    path: &std::path::Path,
    host: &mut plugin_host::PluginHost,
    log_sender: &wasm::logging::channel::LogSender,
    source_registry: &wasm::protocol::PluginSourceRegistry,
) -> anyhow::Result<String> {
    // Open the appropriate source based on path type:
    // .torchsnap files are zip archives, directories use
    // the filesystem directly.
    let source: Arc<dyn wasm::source::PluginSource> = if path.is_dir() {
        Arc::new(wasm::source::DirectorySource::open(path)?)
    } else {
        Arc::new(wasm::source::ArchiveSource::open(path)?)
    };

    let plugin_id = source.manifest().plugin.id.as_str().to_string();
    let wasm_bytes = source.read_wasm()?;
    let instance = runtime.instantiate(&plugin_id, &wasm_bytes)?;
    let manifest = source.manifest().clone();
    let bridge = wasm::bridge::WasmPluginBridge::new(manifest, instance, log_sender.clone());
    host.register(Box::new(bridge));

    // Retain the source in the registry so the protocol
    // handler can serve frontend assets from it.
    source_registry
        .write()
        .expect("source registry not poisoned")
        .insert(plugin_id.clone(), source);

    Ok(plugin_id)
}
