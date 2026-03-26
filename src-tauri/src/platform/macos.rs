// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// macOS Platform Implementation
//
// Launcher panel: converts the Tauri window into a custom
// NSPanel that accepts keyboard input without activating the
// owning process. The previously focused app retains its
// active state while the user types into the launcher.
//
// Tray: uses a template (alpha-mask) icon so macOS can tint
// it to match the menu bar appearance. Left-click toggles
// the launcher; right-click (or ctrl-click) opens the
// context menu.
// =========================================================

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use tauri::Manager as _;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri_nspanel::ManagerExt as _;
use tauri_nspanel::WebviewWindowExt as _;
use tauri_nspanel::objc2_app_kit::NSWindowStyleMask;

use super::app_discovery::{AppDiscovery, DiscoveredApp};
use super::{LauncherPanel, Tray};

// =========================================================
// Custom NSPanel Subclass
// =========================================================

tauri_nspanel::tauri_panel! {
    panel!(TorchsnapLauncherPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
            // Intentionally NOT is_floating_panel: floating panels get
            // auto-dimmed when the owning app is inactive, and ours is
            // non-activating so it would always appear dimmed. The
            // floating window *level* (set separately) handles z-order.
            is_floating_panel: false
        }
    })
}

// =========================================================
// LauncherPanel Implementation
// =========================================================

pub struct MacosLauncherPanel;

impl LauncherPanel for MacosLauncherPanel {
    fn init(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
        let panel = window
            .to_panel::<TorchsnapLauncherPanel>()
            .map_err(|e| anyhow::anyhow!("convert launcher window to NSPanel: {e:?}"))?;

        // Keep the panel visible even when the app is not active.
        // Without this, the non-activating panel would auto-hide
        // when the user clicks another app.
        panel.set_hides_on_deactivate(false);

        // Float above normal windows so the launcher is always
        // reachable, even over fullscreen apps.
        panel.set_level(tauri_nspanel::PanelLevel::Floating.into());

        // Add NonactivatingPanel to the existing style mask rather
        // than replacing it, preserving whatever Tauri configured.
        let ns_panel = panel.as_panel();
        let mask = ns_panel.styleMask();
        ns_panel.setStyleMask(mask | NSWindowStyleMask::NonactivatingPanel);

        // Appear on every Space and over fullscreen apps.
        panel.set_collection_behavior(
            tauri_nspanel::CollectionBehavior::new()
                .can_join_all_spaces()
                .full_screen_auxiliary()
                .into(),
        );

        Ok(())
    }

    fn show(app: &tauri::AppHandle) -> anyhow::Result<()> {
        let panel = app
            .get_webview_panel("main")
            .map_err(|e| anyhow::anyhow!("retrieve launcher panel: {e:?}"))?;
        panel.show_and_make_key();
        Ok(())
    }

    fn hide(app: &tauri::AppHandle) -> anyhow::Result<()> {
        let panel = app
            .get_webview_panel("main")
            .map_err(|e| anyhow::anyhow!("retrieve launcher panel: {e:?}"))?;
        panel.hide();
        Ok(())
    }

    fn is_visible(app: &tauri::AppHandle) -> anyhow::Result<bool> {
        let panel = app
            .get_webview_panel("main")
            .map_err(|e| anyhow::anyhow!("retrieve launcher panel: {e:?}"))?;
        Ok(panel.is_visible())
    }
}

// =========================================================
// Tray Implementation
//
// macOS menu bar convention: the tray icon is a template
// image (alpha mask) so the system applies the correct tint
// for light/dark menu bars automatically. Left-click toggles
// the launcher; the context menu appears on right-click.
// =========================================================

pub struct MacosTray;

impl Tray for MacosTray {
    fn build(
        app: &tauri::App,
        on_toggle: fn(&tauri::AppHandle),
        on_settings: fn(&tauri::AppHandle),
    ) -> anyhow::Result<()> {
        let settings_item = MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)
            .context("create Settings menu item")?;
        let separator = PredefinedMenuItem::separator(app).context("create menu separator")?;
        let quit_item = MenuItem::with_id(app, "quit", "Quit Torchsnap", true, Some("CmdOrCtrl+Q"))
            .context("create Quit menu item")?;
        let menu = Menu::with_items(app, &[&settings_item, &separator, &quit_item])
            .context("build tray menu")?;

        // Template icon: macOS tints the alpha mask to match the
        // current menu bar appearance (light or dark).
        let tray_icon = Image::from_bytes(include_bytes!("../../icons/tray-icon-template.png"))
            .context("load tray icon")?;

        TrayIconBuilder::new()
            .icon(tray_icon)
            .icon_as_template(true)
            .tooltip("Torchsnap")
            .menu(&menu)
            .show_menu_on_left_click(false)
            .on_menu_event(move |app, event| match event.id.as_ref() {
                "settings" => on_settings(app),
                "quit" => app.exit(0),
                _ => {}
            })
            .on_tray_icon_event(move |tray, event| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = event
                {
                    on_toggle(tray.app_handle());
                }
            })
            .build(app)
            .context("build tray icon")?;

        Ok(())
    }
}

// =========================================================
// Application Discovery
//
// Discovers installed applications by querying the Spotlight
// index via the `mdfind` CLI and parsing each app bundle's
// `Info.plist` for display name and visibility metadata.
//
// The Mdfind struct encapsulates the raw subprocess call so
// the discovery logic stays focused on metadata extraction.
// =========================================================

/// Thin wrapper around the macOS `mdfind` Spotlight CLI.
struct Mdfind;

impl Mdfind {
    /// Run a Spotlight query and return one path per result line.
    fn query(predicate: &str) -> anyhow::Result<Vec<PathBuf>> {
        let output = Command::new("mdfind")
            .arg(predicate)
            .output()
            .context("spawn mdfind process")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("mdfind exited with {}: {stderr}", output.status);
        }

        let stdout = String::from_utf8(output.stdout).context("decode mdfind output as UTF-8")?;

        Ok(stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect())
    }
}

/// Check if a plist key is truthy: boolean `true` or string `"1"`.
///
/// macOS plists use both representations for flag keys like
/// `LSUIElement` and `LSBackgroundOnly` depending on the tool
/// that generated the plist.
fn is_plist_truthy(dict: &plist::Dictionary, key: &str) -> bool {
    dict.get(key)
        .is_some_and(|v| v.as_boolean() == Some(true) || v.as_string() == Some("1"))
}

/// Directories where the user (or App Store) explicitly installs apps.
/// Apps here are always included regardless of `LSUIElement` — many
/// legitimate menubar/agent apps (Bartender, Alfred, Yoink, etc.) set
/// `LSUIElement = true` to hide from the dock but are still meant to
/// be launched by the user.
const USER_APP_DIRS: &[&str] = &["/Applications", "/Users"];

/// Whether an app path is inside a user-managed application directory.
fn is_user_installed(path: &Path) -> bool {
    let s = path.to_string_lossy();
    USER_APP_DIRS.iter().any(|prefix| s.starts_with(prefix))
}

/// Read an app bundle's `Info.plist` and extract display name
/// and visibility metadata.
///
/// Returns `None` for system background agents — apps outside
/// user directories that have `LSUIElement` or `LSBackgroundOnly`
/// set. User-installed apps (under `/Applications` or `~/Applications`)
/// are always included since many legitimate menubar apps use these
/// flags to hide from the dock.
fn try_discover_app(path: &Path) -> anyhow::Result<Option<DiscoveredApp>> {
    let plist_path = path.join("Contents/Info.plist");
    let info: plist::Dictionary = plist::from_file(&plist_path).context("read Info.plist")?;

    // Only filter background agents from system directories. Apps
    // in /Applications and ~/Applications are user-chosen and should
    // always appear — even if they set LSUIElement to hide the dock
    // icon (common for menubar-only apps like Bartender, Alfred, etc.).
    if !is_user_installed(path)
        && (is_plist_truthy(&info, "LSUIElement") || is_plist_truthy(&info, "LSBackgroundOnly"))
    {
        return Ok(None);
    }

    // Display name resolution order:
    //   1. CFBundleDisplayName — the localized user-facing name
    //   2. CFBundleName — shorter internal name
    //   3. Filename minus .app — last resort fallback
    let name = info
        .get("CFBundleDisplayName")
        .and_then(|v| v.as_string())
        .or_else(|| info.get("CFBundleName").and_then(|v| v.as_string()))
        .map(String::from)
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        });

    let bundle_id = info
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .map(String::from);

    let id = path.to_string_lossy().into_owned();

    Ok(Some(DiscoveredApp {
        id,
        name,
        path: path.to_owned(),
        bundle_id,
        icon_path: None,
    }))
}

/// Discovers macOS applications via Spotlight (`mdfind`) and
/// `Info.plist` metadata parsing.
pub struct MdfindDiscovery;

impl AppDiscovery for MdfindDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>> {
        let paths = Mdfind::query("kMDItemContentType == 'com.apple.application-bundle'")
            .context("query Spotlight for application bundles")?;

        let mut apps = Vec::with_capacity(paths.len());

        for path in &paths {
            match try_discover_app(path) {
                Ok(Some(app)) => apps.push(app),
                // Filtered out (background app) — silently skip.
                Ok(None) => {}
                // Individual parse failures should not abort the
                // entire discovery. Log and continue.
                Err(e) => {
                    eprintln!("skipping {}: {e:#}", path.display());
                }
            }
        }

        Ok(apps)
    }
}

// =========================================================
// Icon Extraction
//
// Uses NSWorkspace to get the system-composited app icon,
// which handles all icon sources: .icns files, Asset Catalogs
// (.car), and system-provided defaults. The icon is rendered
// at 128×128 and encoded as PNG via NSBitmapImageRep.
// =========================================================

use super::icon_extraction::IconExtractor;

pub struct MacosIconExtractor;

impl IconExtractor for MacosIconExtractor {
    fn extract(&self, app_path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
        use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSWorkspace};
        use objc2_foundation::{NSDictionary, NSSize, NSString};

        let ns_path = NSString::from_str(&app_path.to_string_lossy());

        // All NSWorkspace icon calls are safe to invoke from
        // background threads — they read from the Launch Services
        // icon cache which is thread-safe.
        unsafe {
            let workspace = NSWorkspace::sharedWorkspace();
            let image = workspace.iconForFile(&ns_path);

            // Render at 128×128 points. NSImage renders at the
            // best available representation for this size.
            image.setSize(NSSize::new(128.0, 128.0));

            let Some(tiff_data) = image.TIFFRepresentation() else {
                return Ok(None);
            };

            let Some(bitmap_rep) = NSBitmapImageRep::imageRepWithData(&tiff_data) else {
                return Ok(None);
            };

            let png_data = bitmap_rep
                .representationUsingType_properties(
                    NSBitmapImageFileType::PNG,
                    &NSDictionary::new(),
                )
                .context("encode icon as PNG")?;

            Ok(Some(png_data.to_vec()))
        }
    }
}
