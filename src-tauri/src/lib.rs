// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod control;
mod icons;
mod platform;
mod plugin_host;
mod plugins;
mod search;
mod settings;
mod settings_notifier;
mod storage;

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
// Settings Window
// =========================================================

/// Create, show, and focus the settings window.
///
/// The settings window is created on demand and destroyed when closed
/// to keep memory usage low while it is not visible. On first creation
/// the window stays hidden until the frontend emits `"react-ready"`,
/// preventing a flash of empty content. If the window already exists
/// (e.g. the user triggered "Settings..." twice quickly) it is simply
/// focused.
pub(crate) fn show_settings_window(app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        let _ = app.show();
    }

    // If the window is already alive just bring it to front.
    if let Some(existing) = app.get_webview_window("settings") {
        present_settings_window(&existing, app);
        return;
    }

    // Build the window hidden — the frontend will signal readiness.
    let mut builder =
        WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
            .title("Torchsnap Settings")
            .inner_size(720.0, 520.0)
            .min_inner_size(600.0, 400.0)
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
            eprintln!("failed to create settings window: {e:#}");
            return;
        }
    };

    // Wait for the React frontend to finish its first render before
    // making the window visible. The event listener runs on a
    // background thread, so we dispatch to the main thread since
    // `present_settings_window` accesses the NSWindow handle.
    let handle = app.clone();
    win.once("react-ready", move |_| {
        let inner_handle = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Some(win) = inner_handle.get_webview_window("settings") {
                present_settings_window(&win, &inner_handle);
            }
        });
    });
}

/// Position, show, and focus the settings window on the monitor under
/// the cursor.
fn present_settings_window(win: &tauri::WebviewWindow, app: &tauri::AppHandle) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

        let ns_window = win.ns_window().expect("settings NSWindow handle");
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
// Control API — frontend channel subscription
// =========================================================

/// Called by the frontend at mount time to establish the
/// control channel. The channel is stored in managed state
/// so that control handlers can push commands to it.
#[tauri::command]
fn control_subscribe(
    channel: Channel<control::ControlCommand>,
    app: tauri::AppHandle,
) {
    app.state::<control::ControlChannelState>().set(channel);
}

// =========================================================
// App Entry Point
// =========================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            search::search_query,
            search::search_execute,
            search::plugin_message,
            control_subscribe,
            launcher_hide,
            launcher_set_layout,
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

    let app = builder
        .setup(|app| {
            // =========================================================
            // Global settings defaults
            // =========================================================
            use tauri_plugin_store::StoreExt;

            let store = app.store("settings.json").expect("settings store");
            settings::SettingsInit::from_store(&store, "")
                .ensure("globalShortcut", "CmdOrCtrl+Shift+Space")
                .ensure("mascotMode", "center")
                .ensure("controlChannel.enabled", false)
                .apply(&store, "");

            // =========================================================
            // Settings notifier
            // =========================================================
            let notifier = Arc::new(settings_notifier::SettingsNotifier::new());

            // =========================================================
            // Plugin host
            //
            // Central authority for plugin lifecycle: registration,
            // settings init, parallel setup, shortcut management,
            // search routing, and teardown.
            // =========================================================
            let mut host = plugin_host::PluginHost::new(Arc::clone(&store), Arc::clone(&notifier));
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
            host.register_query(Box::new(plugins::emoji::EmojiPickerPlugin::new()));

            // Settings init + shortcut registration + parallel setup.
            host.initialize_and_start(app.handle());

            let host = Arc::new(host);

            // Spawn the shortcut reactor — watches for settings changes
            // and re-registers all shortcuts when relevant keys change.
            host.start_shortcut_reactor(app.handle());

            app.manage(Arc::clone(&host));

            // =========================================================
            // Settings-changed listener
            //
            // Propagates store changes to watch channels AND signals
            // the shortcut reactor when a relevant key changes.
            // =========================================================
            {
                let notifier = Arc::clone(&notifier);
                let store_for_listener = Arc::clone(&store);
                let host_for_listener = Arc::clone(&host);
                app.listen("settings-changed", move |event: tauri::Event| {
                    #[derive(serde::Deserialize)]
                    struct Payload {
                        key: String,
                    }
                    if let Ok(payload) = serde_json::from_str::<Payload>(event.payload()) {
                        let value = store_for_listener
                            .get(&payload.key)
                            .unwrap_or(serde_json::Value::Null);
                        notifier.notify(&payload.key, value);

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
            control::start_control_server_reactor(
                app.handle(),
                &notifier,
                &store,
            );

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
            PlatformTray::build(app, toggle_launcher_window, show_settings_window)
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
    // On exit, teardown all plugins.
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
            let host = app.state::<Arc<plugin_host::PluginHost>>();
            host.teardown_all();

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
