// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

mod commands;
mod control;
mod entry_store;
mod frecency;
mod gadget_host;
mod gadget_install;
mod gadgets;
mod icons;
mod network;
mod platform;
mod settings;
mod storage;
mod unicode;
mod wasm;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use anyhow::Context;
use tauri::{
    Emitter, Listener, Manager, RunEvent, WebviewUrl, WindowEvent, ipc::Channel,
    webview::WebviewWindowBuilder,
};

#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;

use platform::{
    LauncherPanel as _, PlatformLauncherPanel, PlatformTray, PlatformWindowChrome, Tray as _,
    WindowChrome as _,
};

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
        // briefly to update a collection behavior flag.
        let ns_window: &NSWindow = unsafe { &*(ns_window as *const NSWindow) };
        // OR our flags into whatever collection behavior Tauri
        // configured so we preserve the defaults instead of
        // clobbering them. `FullScreenPrimary` is explicitly
        // added because `toggleFullScreen:` silently no-ops
        // without it, and Tauri's builder does not guarantee
        // the flag is set on windows that use `TitleBarStyle::Overlay`.
        let existing = ns_window.collectionBehavior();
        ns_window.setCollectionBehavior(
            existing
                | NSWindowCollectionBehavior::MoveToActiveSpace
                | NSWindowCollectionBehavior::FullScreenPrimary,
        );
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
    /// When true, the native window controls (close / minimize /
    /// zoom on macOS, equivalent on other platforms) are hidden
    /// after the window is built so the frontend can render its
    /// own title bar. See ADR 0034 for the rationale behind this
    /// approach over `decorations(false)`.
    hide_native_chrome: bool,
}

/// Build an on-demand auxiliary window that stays hidden until the
/// frontend emits `"react-ready"`. If the window already exists it
/// is simply brought to front.
///
/// All work hops to the main thread: window construction
/// (`WebviewWindowBuilder::build`), activation-policy changes, and
/// AppKit-side window manipulation in `present_auxiliary_window` all
/// require the main thread on macOS, and Tauri's `app.show()` does
/// too. Callers can be on any thread — the launcher dispatches
/// `search_execute` on a Tokio blocking worker, the tray menu fires
/// on the main thread itself, and other call sites may follow.
/// Hopping unconditionally inside this function keeps the contract
/// simple instead of asking every caller to remember.
fn show_auxiliary_window(app: &tauri::AppHandle, config: &'static AuxiliaryWindowConfig) {
    let app = app.clone();
    let _ = app.clone().run_on_main_thread(move || {
        show_auxiliary_window_main_thread(&app, config);
    });
}

/// Body of `show_auxiliary_window` running on the main thread.
fn show_auxiliary_window_main_thread(
    app: &tauri::AppHandle,
    config: &'static AuxiliaryWindowConfig,
) {
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
    let builder = WebviewWindowBuilder::new(app, config.label, WebviewUrl::App(config.url.into()))
        .title(config.title)
        .inner_size(config.width, config.height)
        .min_inner_size(config.min_width, config.min_height)
        .resizable(true)
        .visible(false)
        .focused(false)
        .center();

    // Shadowed on macOS to extend the builder without requiring `mut`
    // on platforms where the extension does not apply.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true);

    let win = match builder.build() {
        Ok(w) => w,
        Err(e) => {
            eprintln!("failed to create {} window: {e:#}", config.label);
            return;
        }
    };

    // Tauri v2 has no API to hide only the native title-bar
    // controls (macOS traffic lights / Windows caption buttons)
    // while keeping the rest of the native window intact. Using
    // `decorations(false)` would remove too much — rounded
    // corners, shadow, edge-drag niceties — with no clean route
    // back on macOS. The platform abstraction below hides only
    // the buttons via public AppKit API. ADR 0034 documents the
    // reasoning and the alternatives we considered.
    if config.hide_native_chrome
        && let Err(e) = PlatformWindowChrome::hide_controls(&win)
    {
        eprintln!("failed to hide {} window controls: {e:#}", config.label);
    }

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
    hide_native_chrome: true,
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
    hide_native_chrome: true,
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

/// Initiate a launcher dismiss.
///
/// On macOS / Windows the launcher can be hidden immediately:
/// `WKWebView` / `WebView2` keep compositing while their host
/// window is invisible, so the next show simply reveals a buffer
/// that already matches the current DOM.
///
/// On Linux / WebKitGTK that is not the case — an unmapped
/// `GtkWindow`'s webview does not produce new frames, so the
/// swapchain presents the pre-hide frame on re-map. If the
/// frontend has not had a chance to blank its content first, the
/// user sees a flash of the previous launcher state (old query,
/// stale results, previous mascot) before React's post-show
/// commit reaches the screen.
///
/// To cooperate with that constraint we emit a
/// `launcher-dismiss-requested` event and let the frontend drive
/// the hide itself via the `launcher_hide` command after it has
/// made the last composited frame blank. See
/// `src/launcher/visibility.ts` for the frontend side.
pub(crate) fn request_launcher_dismiss(app: &tauri::AppHandle) {
    #[cfg(not(target_os = "linux"))]
    {
        hide_launcher(app);
    }

    #[cfg(target_os = "linux")]
    {
        if let Err(e) = app.emit("launcher-dismiss-requested", ()) {
            // If the event bus itself is broken the frontend will
            // never hide the launcher. Fall back to an immediate
            // hide so the window does not get stuck open — the
            // stale-frame flash is preferable to a launcher that
            // won't close.
            eprintln!("failed to emit launcher-dismiss-requested: {e:#}");
            hide_launcher(app);
        }
    }
}

/// Tauri command so the frontend can hide the launcher through
/// the same path as the hotkey toggle and control API, ensuring
/// the window shrink always happens.
#[tauri::command]
fn launcher_hide(app: tauri::AppHandle) {
    hide_launcher(&app);
}

/// Show the launcher and, on Linux, tell the frontend it happened.
///
/// The `launcher-shown` event is the deterministic "the launcher
/// is now visible" signal the frontend uses to restore `#root`'s
/// visibility after the WebKitGTK blanking dance. `tauri://focus`
/// alone is unreliable because Mutter's focus-stealing prevention
/// can deny a programmatic `set_focus` on Wayland — the window
/// shows up but the focus event never fires, leaving the launcher
/// stuck with `visibility: hidden`.
pub(crate) fn show_launcher(app: &tauri::AppHandle) -> anyhow::Result<()> {
    PlatformLauncherPanel::show(app)?;
    #[cfg(target_os = "linux")]
    {
        if let Err(e) = app.emit("launcher-shown", ()) {
            eprintln!("failed to emit launcher-shown: {e:#}");
        }
    }
    Ok(())
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
        if let Err(e) = show_launcher(&app) {
            eprintln!("failed to show launcher (deferred): {e:#}");
        }
    }
}

pub(crate) fn toggle_launcher_window(app: &tauri::AppHandle) {
    let is_visible = PlatformLauncherPanel::is_visible(app).unwrap_or(false);

    if is_visible {
        request_launcher_dismiss(app);
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

    if let Err(e) = show_launcher(app) {
        eprintln!("failed to show launcher: {e:#}");
    }
}

// =========================================================
// Frecency Commands
// =========================================================

/// Both values are embedded at compile time via `env!()`.
#[tauri::command]
fn build_info() -> serde_json::Value {
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "gitHash": env!("GIT_HASH"),
    })
}

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
// WASM Gadget Manifests
// =========================================================

/// Returns the full manifest for every loaded WASM gadget.
/// Called once per webview at startup to register dynamic
/// gadget components and settings entries.
#[tauri::command]
fn wasm_gadgets(
    registry: tauri::State<'_, wasm::protocol::GadgetSourceRegistry>,
) -> Vec<wasm::manifest::Manifest> {
    let sources = registry.read().expect("source registry not poisoned");
    sources.values().map(|s| s.manifest().clone()).collect()
}

/// Return the origin of every registered gadget (native,
/// bundled WASM, user-installed WASM, or dev-path WASM).
/// Consumed by the Gadgets settings panel to render source
/// badges and gate the uninstall action to `user` gadgets.
#[tauri::command]
fn gadget_sources(
    host: tauri::State<'_, Arc<gadget_host::GadgetHost>>,
) -> std::collections::HashMap<String, wasm::source::GadgetSourceKind> {
    host.gadget_sources()
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
            commands::search,
            commands::search_execute,
            commands::gadget_message,
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
            wasm_gadgets,
            gadget_sources,
            gadget_install::install_gadget_archive,
            gadget_install::uninstall_user_gadget,
            build_info,
        ])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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

    // Shared registry mapping gadget IDs to their sources.
    // The protocol handler reads from these to serve frontend
    // assets; gadget loading populates the registry.
    let source_registry = wasm::protocol::new_registry();
    let builder = wasm::protocol::register_gadget_protocol(builder, Arc::clone(&source_registry));

    // Host favicon URI scheme. Custom schemes must be registered
    // on the builder before `.setup()` runs, but
    // `WebsiteMetadataService` (which owns the favicon store) is
    // constructed inside `.setup()` — the `OnceLock` registry
    // bridges the gap, populated below once the service exists.
    let favicon_registry = network::website_metadata::protocol::new_registry();
    let builder = network::website_metadata::protocol::register_favicon_protocol(
        builder,
        Arc::clone(&favicon_registry),
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
            let notifier = Arc::new(settings::notifier::SettingsNotifier::new());

            // =========================================================
            // Frecency store
            // =========================================================
            let app_data_dir = app.path().app_data_dir().context("resolve app data dir")?;
            let frecency_store = frecency::FrecencyStore::open(&app_data_dir, &notifier, &store)
                .context("initialize frecency store")?;
            let frecency_store = Arc::new(frecency_store);

            // =========================================================
            // Gadget host
            //
            // Central authority for gadget lifecycle: registration,
            // settings init, parallel enable, shortcut management,
            // search routing, and shutdown.
            // =========================================================
            let mut host =
                gadget_host::GadgetHost::new(Arc::clone(&store), Arc::clone(&frecency_store));
            // All built-in gadgets are native Rust code compiled
            // into the binary — tag them `Builtin`. The Gadgets
            // settings panel uses this to suppress the uninstall
            // action for built-ins.
            host.register(
                gadgets::commands::BuiltInCommandsGadget,
                wasm::source::GadgetSourceKind::Builtin,
            );
            host.register(
                gadgets::system_commands::SystemCommandsGadget::new(),
                wasm::source::GadgetSourceKind::Builtin,
            );

            let icon_cache_dir = app
                .path()
                .app_cache_dir()
                .context("resolve app cache dir")?
                .join("icons");
            let icon_cache = Arc::new(icons::IconCache::new(icon_cache_dir));
            host.register(
                gadgets::app_launcher::AppLauncherGadget::new(
                    platform::PlatformAppDiscovery,
                    Arc::clone(&icon_cache),
                ),
                wasm::source::GadgetSourceKind::Builtin,
            );
            host.register(
                gadgets::system_preferences::SystemPreferencesGadget::new(
                    platform::PlatformSettingsDiscovery,
                    Arc::clone(&icon_cache),
                ),
                wasm::source::GadgetSourceKind::Builtin,
            );
            host.register(
                gadgets::clipboard::ClipboardGadget::new(
                    platform::PlatformClipboard,
                ),
                wasm::source::GadgetSourceKind::Builtin,
            );

            // =========================================================
            // Website metadata service
            //
            // Shared service for fetching and caching website metadata
            // (title, description, favicon). Gadgets that need domain
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

            // Now that the service exists, hand its favicon store
            // to the host-favicon protocol handler registered on
            // the builder above. `OnceLock::set` only fails if
            // already initialized — which can't happen here since
            // `setup` runs exactly once.
            favicon_registry
                .set(metadata_service.favicon_store())
                .map_err(|_| anyhow::anyhow!("favicon registry already populated"))?;

            // =========================================================
            // Logging system
            //
            // Structured logging for WASM gadgets. Started before
            // gadget loading so that compilation and instantiation
            // timing is captured from the very first gadget.
            // =========================================================
            let logging_system = Arc::new(wasm::logging::channel::LoggingSystem::start());
            let log_ctx = logging_system.context();
            let log_sender = log_ctx.sender.clone();
            let span_registry = Arc::clone(logging_system.span_registry());

            // =========================================================
            // WASM gadgets
            //
            // Load gadgets from the `gadgets/` development directory.
            // Each subdirectory with a manifest.toml is loaded as a
            // DirectorySource, instantiated via wasmtime, and bridged
            // to the native Gadget trait.
            //
            // Roots scanned (see `wasm::discovery` for details):
            //
            // - `resource_dir/gadgets/` — bundled System gadgets
            //   (release + debug if the resource dir exists).
            // - `CARGO_MANIFEST_DIR/../gadgets/` — Dev gadgets
            //   (debug builds only, stripped in release).
            // - `app_data_dir/gadgets/` — User-installed gadgets.
            // =========================================================
            let resource_dir = app.path().resource_dir().ok();
            match load_wasm_gadgets(
                &mut host,
                &log_ctx,
                &source_registry,
                &app_data_dir,
                resource_dir.as_deref(),
                Arc::clone(&metadata_service),
            ) {
                Ok(count) => {
                    if count > 0 {
                        log_sender.send(wasm::logging::LogItem {
                            seq: 0,
                            timestamp: std::time::SystemTime::now(),
                            source: wasm::logging::LogSource::Host,
                            kind: wasm::logging::LogItemKind::Message {
                                level: wasm::logging::LogLevel::Info,
                                message: format!("Loaded {count} WASM gadget(s)"),
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
                            message: format!("Failed to initialize WASM gadgets: {e:#}"),
                            metadata: vec![],
                            span_id: None,
                        },
                    });
                }
            }

            // Settings init + shortcut registration + parallel setup.
            let prov_ctx = gadgets::ProvisioningContext {
                app: app.handle().clone(),
                store: Arc::clone(&store),
                frecency: Arc::clone(&frecency_store),
                icon_cache: Arc::clone(&icon_cache),
                metadata_service: Arc::clone(&metadata_service),
            };
            host.initialize_and_start(prov_ctx);

            let host = Arc::new(host);

            // Spawn the shortcut reactor — watches for settings changes
            // and re-registers all shortcuts when relevant keys change.
            host.start_shortcut_reactor(app.handle());

            app.manage(Arc::clone(&host));
            app.manage(Arc::clone(&frecency_store));
            app.manage(Arc::clone(&metadata_service));
            app.manage(Arc::clone(&logging_system));
            app.manage(Arc::clone(&span_registry));
            app.manage(source_registry);

            // =========================================================
            // Settings-changed listener
            //
            // Propagates store changes to:
            // 1. Watch channels (SettingsNotifier) for non-gadget
            //    SettingsWatch subscribers (FrecencyStore, control, etc.)
            // 2. Host-managed gadget lifecycle (enable/disable) and
            //    settings dispatch (setting_changed) via
            //    CoalescingDispatcher
            // 3. Shortcut reactor for re-registration
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

                        notifier.notify(&payload.key, value.clone());

                        host_for_listener.handle_setting_changed(
                            &payload.key,
                            value,
                        );

                        // Signal shortcut re-registration if the changed
                        // key affects shortcuts or gadget enabled state.
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
            PlatformTray::build(
                app,
                toggle_launcher_window,
                show_settings_window,
                show_devtools_window,
            )
            .context("build platform tray")?;

            // =========================================================
            // Preload windows
            // =========================================================

            // The launcher window is normally built hidden and stays
            // hidden until the hotkey or control API asks for it.
            // That relies on the webview running JavaScript while the
            // host window is unmapped — which WKWebView does on macOS
            // but WebKitGTK does not: on Linux a hidden GtkWindow
            // never realizes the webview, so React never mounts and
            // the `launcher_set_layout` handshake that gates every
            // show path can never complete. Starting visible on
            // Linux side-steps the deadlock; once the user dismisses
            // the launcher the first time, the webview has been
            // realized and normal hide/show cycling works for the
            // rest of the session.
            let launcher_builder =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("launcher.html".into()))
                    .transparent(true)
                    .decorations(false)
                    .shadow(false)
                    .focused(false)
                    .title("");

            #[cfg(not(target_os = "linux"))]
            let launcher_builder = launcher_builder.visible(false);

            let launcher_win = launcher_builder.build().context("create launcher window")?;

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
    // On exit, disable all gadgets.
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

            let host = app.state::<Arc<gadget_host::GadgetHost>>();
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
// WASM Gadget Loader
//
// Scans every configured search root (see
// `wasm::discovery`) for gadget archives and directory-form
// gadgets, instantiates each one once, and registers it with
// the `GadgetHost` tagged with its `GadgetSourceKind`.
//
// Cross-root collision rule: the first root that registers a
// given gadget id wins. Since `enumerate_search_roots` orders
// the roots System → Dev → User, a bundled gadget always
// shadows a user copy that happens to share its id. A
// warning is logged for the skipped copy so the user can see
// why their install did not take effect.
//
// Per-root precedence (archive-over-directory) is owned by
// `wasm::discovery::scan_gadget_entries`.
// =========================================================

fn load_wasm_gadgets(
    host: &mut gadget_host::GadgetHost,
    log_ctx: &wasm::logging::channel::LogContext,
    source_registry: &wasm::protocol::GadgetSourceRegistry,
    app_data_dir: &std::path::Path,
    resource_dir: Option<&std::path::Path>,
    metadata_service: Arc<network::website_metadata::WebsiteMetadataService>,
) -> anyhow::Result<usize> {
    let runtime: Arc<wasm::runtime::WasmRuntime> = wasm::runtime::WasmRuntime::new()?;
    let log_sender = &log_ctx.sender;

    let roots = wasm::discovery::enumerate_search_roots(resource_dir, app_data_dir);

    let mut loaded_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut count: usize = 0;

    for (source_kind, root) in roots {
        for path in wasm::discovery::scan_gadget_entries(&root) {
            // Open the source once. On any error — corrupt
            // archive, missing manifest, failing path guard —
            // log and move on so one broken gadget does not
            // prevent the rest from loading.
            let source: Arc<dyn wasm::source::GadgetSource + Send + Sync> =
                match open_gadget_source(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        log_loader_error(log_sender, &path, source_kind, &e);
                        continue;
                    }
                };

            // Dedup across roots: the System/Dev/User ordering
            // means the first registration of a given id wins.
            // Subsequent appearances are skipped with a warning
            // so an install-time collision bug surfaces visibly
            // instead of silently.
            let gadget_id = source.manifest().gadget.id.as_str().to_string();
            if !loaded_ids.insert(gadget_id.clone()) {
                log_sender.send(wasm::logging::LogItem {
                    seq: 0,
                    timestamp: std::time::SystemTime::now(),
                    source: wasm::logging::LogSource::Host,
                    kind: wasm::logging::LogItemKind::Message {
                        level: wasm::logging::LogLevel::Warn,
                        message: format!(
                            "gadget id `{gadget_id}` already loaded from an earlier root — skipping {}",
                            path.display()
                        ),
                        metadata: vec![
                            ("gadget_id".to_string(), gadget_id.clone()),
                            ("source".to_string(), format!("{source_kind:?}")),
                            ("skipped_path".to_string(), path.display().to_string()),
                        ],
                        span_id: None,
                    },
                });
                continue;
            }

            match load_single_wasm_gadget(
                Arc::clone(&runtime),
                source,
                source_kind,
                host,
                log_ctx,
                source_registry,
                app_data_dir,
                Some(Arc::clone(&metadata_service)),
            ) {
                Ok(gadget_id) => {
                    log_sender.send(wasm::logging::LogItem {
                        seq: 0,
                        timestamp: std::time::SystemTime::now(),
                        source: wasm::logging::LogSource::Host,
                        kind: wasm::logging::LogItemKind::Message {
                            level: wasm::logging::LogLevel::Info,
                            message: format!("Loaded gadget: {gadget_id} ({source_kind:?})"),
                            metadata: vec![
                                ("gadget_id".to_string(), gadget_id),
                                ("source".to_string(), format!("{source_kind:?}")),
                            ],
                            span_id: None,
                        },
                    });
                    count += 1;
                }
                Err(e) => {
                    // Removing from loaded_ids so a fallback
                    // copy in a later root could still get a
                    // chance. The registration itself is already
                    // rolled back — `register` is the last step
                    // in `load_single_wasm_gadget` and the error
                    // happens before it.
                    loaded_ids.remove(&gadget_id);
                    log_loader_error(log_sender, &path, source_kind, &e);
                }
            }
        }
    }

    Ok(count)
}

/// Open a gadget source from a filesystem path, choosing
/// `DirectorySource` vs `ArchiveSource` by directory-ness.
/// Split out of `load_wasm_gadgets` so the collision check
/// can run against the manifest id *before* the expensive
/// WASM compile in `WasmGadgetBridge::new`.
fn open_gadget_source(
    path: &std::path::Path,
) -> anyhow::Result<Arc<dyn wasm::source::GadgetSource + Send + Sync>> {
    if path.is_dir() {
        Ok(Arc::new(wasm::source::DirectorySource::open(path)?))
    } else {
        Ok(Arc::new(wasm::source::ArchiveSource::open(path)?))
    }
}

/// Emit a load-failure log entry. Used for both source-open
/// and bridge-construction failures so the two failure modes
/// surface identically in the devtools log viewer.
fn log_loader_error(
    log_sender: &wasm::logging::channel::LogSender,
    path: &std::path::Path,
    source_kind: wasm::source::GadgetSourceKind,
    error: &anyhow::Error,
) {
    log_sender.send(wasm::logging::LogItem {
        seq: 0,
        timestamp: std::time::SystemTime::now(),
        source: wasm::logging::LogSource::Host,
        kind: wasm::logging::LogItemKind::Message {
            level: wasm::logging::LogLevel::Error,
            message: format!("Failed to load {}: {error:#}", path.display()),
            metadata: vec![
                ("source".to_string(), format!("{source_kind:?}")),
                ("path".to_string(), path.display().to_string()),
            ],
            span_id: None,
        },
    });
}

fn load_single_wasm_gadget(
    runtime: Arc<wasm::runtime::WasmRuntime>,
    source: Arc<dyn wasm::source::GadgetSource + Send + Sync>,
    source_kind: wasm::source::GadgetSourceKind,
    host: &mut gadget_host::GadgetHost,
    log_ctx: &wasm::logging::channel::LogContext,
    source_registry: &wasm::protocol::GadgetSourceRegistry,
    app_data_dir: &std::path::Path,
    metadata_service: Option<Arc<network::website_metadata::WebsiteMetadataService>>,
) -> anyhow::Result<String> {
    let gadget_id = source.manifest().gadget.id.as_str().to_string();
    let manifest = source.manifest().clone();
    let bridge = wasm::bridge::WasmGadgetBridge::new(
        manifest,
        runtime,
        log_ctx.clone(),
        Arc::clone(&source),
        app_data_dir,
        metadata_service,
    )?;
    host.register(bridge, source_kind);

    // Retain the source in the registry so the protocol
    // handler can serve frontend assets from it.
    source_registry
        .write()
        .expect("source registry not poisoned")
        .insert(gadget_id.clone(), source);

    Ok(gadget_id)
}
