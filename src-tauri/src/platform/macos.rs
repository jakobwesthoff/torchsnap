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

/// Whether an app path is inside an Applications directory.
///
/// Matches any path containing `/Applications/` — this covers
/// `/Applications/`, `/System/Applications/`, `~/Applications/`,
/// and any other location following the macOS convention.
fn is_in_applications_dir(path: &Path) -> bool {
    path.to_string_lossy().contains("/Applications/")
}

/// Read an app bundle's `Info.plist` and extract display name
/// and metadata for a user-facing application.
///
/// The caller is responsible for pre-filtering paths to allowed
/// directories. This function only reads metadata — it does not
/// filter by directory.
fn try_discover_app(path: &Path) -> anyhow::Result<Option<DiscoveredApp>> {
    let plist_path = path.join("Contents/Info.plist");
    let info: plist::Dictionary = plist::from_file(&plist_path).context("read Info.plist")?;

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
            // Only index apps in known application directories.
            // This skips system agents in /System/Library/CoreServices/,
            // framework helpers, and other non-user-facing bundles.
            if !is_in_applications_dir(path) {
                continue;
            }

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
// (.car), and system-provided defaults.
//
// macOS internally represents icons with 16-bit float samples,
// which neither `TIFFRepresentation` nor `CGImageForProposedRect`
// will convert for us — both return 16-bit float data that the
// `image` crate cannot decode.
//
// To get clean 8-bit RGBA pixels, we draw the CGImage into a
// `CGBitmapContext` configured for 8-bit premultiplied RGBA.
// Core Graphics handles the float→int conversion (and any
// color space mapping) during the draw call. We then read
// the raw 8-bit pixel data directly from the bitmap context
// and un-premultiply alpha on the Rust side.
//
// The bitmap context is created at the full CGImage resolution.
// The downstream `icon_processing::process_icon` handles
// resizing to the final target dimensions (e.g. 256×256).
// =========================================================

use super::icon_extraction::IconExtractor;

/// Pixel format flag for `CGBitmapContextCreate`:
/// premultiplied alpha in the last component (RGBA layout).
const K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST: u32 = 1;

pub struct MacosIconExtractor;

impl IconExtractor for MacosIconExtractor {
    fn extract(&self, app_path: &Path) -> anyhow::Result<Option<image::DynamicImage>> {
        use objc2_app_kit::NSWorkspace;
        use objc2_core_foundation::{CGPoint, CGRect, CGSize};
        use objc2_core_graphics::{
            CGBitmapContextCreate, CGBitmapContextGetData, CGColorSpace, CGContext, CGImage,
        };
        use objc2_foundation::NSString;

        let ns_path = NSString::from_str(&app_path.to_string_lossy());

        let workspace = NSWorkspace::sharedWorkspace();
        let ns_image = workspace.iconForFile(&ns_path);

        // Obtain a CGImage from the NSImage. The CGImage may use
        // 16-bit float components, any alpha layout, and any color
        // space — we don't care because the bitmap context draw
        // will normalise everything.
        let Some(cg_image) = (unsafe {
            ns_image.CGImageForProposedRect_context_hints(
                std::ptr::null_mut(),
                None,
                None,
            )
        }) else {
            return Ok(None);
        };

        let width = CGImage::width(Some(&cg_image));
        let height = CGImage::height(Some(&cg_image));

        if width == 0 || height == 0 {
            return Ok(None);
        }

        let bytes_per_row = width * 4;

        // Create an 8-bit RGBA bitmap context. Core Graphics will
        // convert the source image's pixel format (including 16-bit
        // float) to 8-bit integer during the draw call.
        let color_space =
            CGColorSpace::new_device_rgb().context("create device RGB color space")?;

        // SAFETY: Passing null for `data` makes CG allocate its own
        // buffer. The color space, dimensions, and bitmap info are
        // valid for an 8-bit RGBA context.
        let ctx = unsafe {
            CGBitmapContextCreate(
                std::ptr::null_mut(),
                width,
                height,
                8,
                bytes_per_row,
                Some(&color_space),
                K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST,
            )
        }
        .context("create bitmap context")?;

        // Draw the source CGImage into the bitmap context, filling
        // the entire area. CG handles format conversion, color space
        // mapping, and compositing.
        let dest_rect = CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize {
                width: width as f64,
                height: height as f64,
            },
        };
        CGContext::draw_image(Some(&ctx), dest_rect, Some(&cg_image));

        // Read the raw 8-bit RGBA pixel data from the bitmap context.
        let data_ptr = CGBitmapContextGetData(Some(&ctx));
        anyhow::ensure!(!data_ptr.is_null(), "bitmap context data pointer is null");

        let total_bytes = height * bytes_per_row;
        // SAFETY: The bitmap context owns the buffer, which remains
        // valid while `ctx` is alive. We copy it out immediately.
        let premultiplied =
            unsafe { std::slice::from_raw_parts(data_ptr as *const u8, total_bytes) };

        // Un-premultiply alpha. The bitmap context produces
        // premultiplied RGBA, but the `image` crate and WebP
        // encoder expect straight (non-premultiplied) alpha.
        let mut rgba = Vec::with_capacity(total_bytes);
        for chunk in premultiplied.chunks_exact(4) {
            let (r, g, b, a) = (chunk[0], chunk[1], chunk[2], chunk[3]);

            let (r, g, b) = if a > 0 && a < 255 {
                let af = a as f32 / 255.0;
                (
                    (r as f32 / af).min(255.0) as u8,
                    (g as f32 / af).min(255.0) as u8,
                    (b as f32 / af).min(255.0) as u8,
                )
            } else {
                (r, g, b)
            };

            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }

        let img = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
            .context("construct RgbaImage from bitmap context pixel data")?;

        Ok(Some(image::DynamicImage::ImageRgba8(img)))
    }
}
