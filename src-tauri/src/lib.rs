mod platform;

use anyhow::Context;
use tauri::{
    Manager, RunEvent, WebviewUrl, WindowEvent,
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::WebviewWindowBuilder,
};

#[cfg(target_os = "macos")]
use tauri::TitleBarStyle;

use platform::{LauncherPanel as _, PlatformLauncherPanel};

// =========================================================
// Settings Window
// =========================================================

/// Show and focus the pre-created settings window.
fn show_settings_window(app: &tauri::AppHandle) {
    // Activate the app so the window actually comes to the foreground.
    // Without this, Accessory-policy apps require a second click because
    // the first click only activates the process.
    #[cfg(target_os = "macos")]
    {
        let _ = app.set_activation_policy(tauri::ActivationPolicy::Accessory);
        let _ = app.show();
    }

    if let Some(win) = app.get_webview_window("settings") {
        // Move the window to the currently active Space so it doesn't
        // pull the user back to the Space where it was originally created.
        //
        // Tauri only exposes `set_visible_on_all_workspaces()`, which maps
        // to `CanJoinAllSpaces` (pins the window to every Space at once).
        // We need `MoveToActiveSpace` instead (window follows the user to
        // whichever Space they're on). Neither Tauri nor tao abstract this
        // flag, so we go through the raw NSWindow pointer.
        #[cfg(target_os = "macos")]
        {
            use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

            let ns_window = win.ns_window().expect("settings NSWindow handle");
            let ns_window: &NSWindow = unsafe { &*(ns_window as *const NSWindow) };
            ns_window.setCollectionBehavior(NSWindowCollectionBehavior::MoveToActiveSpace);
        }

        // Center the settings window on the monitor the cursor is on,
        // so it appears on the screen the user is currently working on
        // rather than wherever it was initially created.
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
}

// =========================================================
// Launcher Window
// =========================================================

/// Find the monitor the cursor is currently on, falling back to the
/// primary monitor.
fn monitor_under_cursor(app: &tauri::AppHandle) -> Option<tauri::Monitor> {
    let cursor = app.cursor_position().ok()?;

    // `cursor_position()` returns physical pixel coordinates, but
    // `monitor_from_point()` expects logical coordinates. We find
    // the matching monitor by iterating the monitor list manually,
    // converting each monitor's physical bounds to the cursor's
    // coordinate space for a hit-test.
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

/// Position the launcher to cover the monitor the cursor is on.
///
/// Uses physical cursor coordinates to find the matching monitor,
/// then sets the window's logical size and position to cover it.
fn position_launcher_on_cursor_monitor(app: &tauri::AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };

    if let Some(monitor) = monitor_under_cursor(app) {
        let size = monitor.size();
        let pos = monitor.position();
        let scale = monitor.scale_factor();

        let logical_width = size.width as f64 / scale;
        let logical_height = size.height as f64 / scale;

        let _ = win.set_size(tauri::LogicalSize::new(logical_width, logical_height));
        let _ = win.set_position(tauri::LogicalPosition::new(
            pos.x as f64 / scale,
            pos.y as f64 / scale,
        ));
    }
}

/// Toggle the launcher overlay on the monitor where the cursor currently is.
fn toggle_launcher_window(app: &tauri::AppHandle) {
    let is_visible = PlatformLauncherPanel::is_visible(app).unwrap_or(false);

    if is_visible {
        if let Err(e) = PlatformLauncherPanel::hide(app) {
            eprintln!("failed to hide launcher: {e:#}");
        }
        return;
    }

    position_launcher_on_cursor_monitor(app);

    if let Err(e) = PlatformLauncherPanel::show(app) {
        eprintln!("failed to show launcher: {e:#}");
    }
}

// =========================================================
// Global Shortcut
// =========================================================

const DEFAULT_SHORTCUT: &str = "CmdOrCtrl+Shift+Space";

/// Read the stored shortcut from the plugin-store, falling back to the
/// compile-time default when no user override exists.
fn read_shortcut(app: &tauri::AppHandle) -> String {
    use tauri_plugin_store::StoreExt;

    let store = app.store("settings.json").expect("settings store");
    store
        .get("globalShortcut")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string())
}

/// Re-register the global shortcut at runtime. Unregisters all existing
/// shortcuts first, then registers the new one with the real toggle
/// handler. Called from the settings UI when the user changes the
/// shortcut — the JS side persists the value to the store separately.
#[tauri::command]
fn update_global_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let parsed = shortcut
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| format!("invalid shortcut: {e}"))?;

    // Unregister all existing shortcuts so the old one is removed.
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("failed to unregister shortcuts: {e}"))?;

    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(parsed, move |_app, _shortcut, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                toggle_launcher_window(&handle);
            }
        })
        .map_err(|e| format!("failed to register shortcut: {e}"))?;

    Ok(())
}

// =========================================================
// App Entry Point
// =========================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![update_global_shortcut])
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_process::init());

    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    let app = builder
        .setup(|app| {
            // =========================================================
            // Hide dock icon (macOS only)
            //
            // Torchsnap is a menubar-only app on macOS — no dock icon
            // or app switcher entry. On Linux/Windows, the app will
            // appear in the taskbar normally.
            // =========================================================
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            // =========================================================
            // Tray icon with context menu
            // =========================================================
            let settings_item =
                MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)
                    .context("create Settings menu item")?;
            let separator = PredefinedMenuItem::separator(app).context("create menu separator")?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Quit Torchsnap", true, Some("CmdOrCtrl+Q"))
                    .context("create Quit menu item")?;
            let menu = Menu::with_items(app, &[&settings_item, &separator, &quit_item])
                .context("build tray menu")?;

            let tray_icon = Image::from_bytes(include_bytes!("../icons/tray-icon-template.png"))
                .context("load tray icon")?;

            TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Torchsnap")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        show_settings_window(app);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_launcher_window(tray.app_handle());
                    }
                })
                .build(app)
                .context("build tray icon")?;

            // =========================================================
            // Preload windows
            //
            // Both windows are created hidden at startup. This avoids
            // the flash / delay that comes from creating a webview on
            // demand when the user triggers the shortcut or opens
            // settings.
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

            // Platform-specific panel initialization (NSPanel on macOS,
            // no-op on other platforms).
            PlatformLauncherPanel::init(&launcher_win)
                .context("initialize platform launcher panel")?;

            let mut settings_builder =
                WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
                    .title("Torchsnap Settings")
                    .inner_size(480.0, 600.0)
                    .resizable(false)
                    .visible(false)
                    .focused(false)
                    .center();

            #[cfg(target_os = "macos")]
            {
                settings_builder = settings_builder
                    .title_bar_style(TitleBarStyle::Overlay)
                    .hidden_title(true);
            }

            settings_builder.build().context("create settings window")?;

            // =========================================================
            // Global shortcut
            //
            // Register the user's configured shortcut (or the default).
            // The handler toggles the launcher overlay directly.
            // =========================================================
            let shortcut = read_shortcut(app.handle());

            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let handle = app.handle().clone();
            app.global_shortcut()
                .on_shortcut(
                    shortcut
                        .parse::<tauri_plugin_global_shortcut::Shortcut>()
                        .expect("valid shortcut string"),
                    move |_app, _shortcut, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            toggle_launcher_window(&handle);
                        }
                    },
                )
                .context("register global shortcut")?;

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // Intercept window close: hide instead of destroying, so the
    // menubar app keeps running.
    app.run(|app, event| {
        if let RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } = &event
            && (label == "main" || label == "settings")
        {
            api.prevent_close();
            if let Some(win) = app.get_webview_window(label) {
                let _ = win.hide();
            }
        }
    });
}
