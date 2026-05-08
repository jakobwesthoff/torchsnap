// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings Discovery (macOS)
//
// Discovers System Settings panes by scanning the ExtensionKit
// extensions directory for `.appex` bundles that declare support
// for the `x-apple.systempreferences:` URL scheme.
//
// Each bundle's `Info.plist` provides the bundle identifier
// (used to construct the URL), the SF Symbol name (used for
// icon rendering), and the `InfoPlist.loctable` provides
// localized display names.
// =========================================================

use std::fs;
use std::path::Path;

use anyhow::Context;

use super::cgimage_conversion::nsworkspace_icon_for_file;
use crate::platform::settings_discovery::{SettingsDiscovery, SettingsPane};

/// Directory where macOS installs Settings extension bundles.
const EXTENSIONS_DIR: &str = "/System/Library/ExtensionKit/Extensions";

pub struct MacosSettingsDiscovery;

impl MacosSettingsDiscovery {
    /// Format the System Settings deep-link URL for a pane.
    pub fn pane_url(pane_id: &str) -> String {
        format!("x-apple.systempreferences:{pane_id}")
    }
}

impl SettingsDiscovery for MacosSettingsDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<SettingsPane>> {
        let extensions_dir = Path::new(EXTENSIONS_DIR);

        let entries =
            fs::read_dir(extensions_dir).context("read ExtensionKit extensions directory")?;

        let system_lang = resolve_system_language();

        let mut panes = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();

            // Only process .appex bundles.
            let is_appex = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext == "appex");
            if !is_appex {
                continue;
            }

            match try_discover_pane(&path, &system_lang) {
                Ok(Some(pane)) => panes.push(pane),
                Ok(None) => {}
                Err(e) => {
                    eprintln!("skip settings extension {}: {e:#}", path.display());
                }
            }
        }

        Ok(panes)
    }

    fn icon(&self, pane: &SettingsPane) -> anyhow::Result<Option<image::DynamicImage>> {
        match &pane.bundle_path {
            Some(path) => nsworkspace_icon_for_file(path),
            None => Ok(None),
        }
    }
}

// =========================================================
// Pane Discovery
// =========================================================

/// Try to extract a settings pane from an `.appex` bundle.
///
/// Returns `Ok(None)` if the bundle doesn't support the
/// `x-apple.systempreferences:` URL scheme (i.e., it's not a
/// settings pane we can open).
fn try_discover_pane(appex_path: &Path, system_lang: &str) -> anyhow::Result<Option<SettingsPane>> {
    let plist_path = appex_path.join("Contents/Info.plist");
    let info: plist::Dictionary = plist::from_file(&plist_path).context("read Info.plist")?;

    // -------------------------------------------------------
    // Check for URL scheme support
    // -------------------------------------------------------

    let allows_url_scheme = info
        .get("EXAppExtensionAttributes")
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("SettingsExtensionAttributes"))
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("allowsXAppleSystemPreferencesURLScheme"))
        .and_then(|v| v.as_boolean())
        .unwrap_or(false);

    if !allows_url_scheme {
        return Ok(None);
    }

    // -------------------------------------------------------
    // Bundle identifier (required for opening)
    // -------------------------------------------------------

    let bundle_id = info
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .map(String::from)
        .context("missing CFBundleIdentifier")?;

    // -------------------------------------------------------
    // Localized display name
    //
    // Fallback chain:
    //   1. InfoPlist.loctable → system language → CFBundleDisplayName
    //   2. InfoPlist.loctable → "en" → CFBundleDisplayName
    //   3. Info.plist → CFBundleDisplayName
    //   4. Info.plist → CFBundleName
    //   5. .appex filename stem
    // -------------------------------------------------------

    let name = resolve_display_name(appex_path, &info, system_lang);

    Ok(Some(SettingsPane {
        id: bundle_id,
        name,
        bundle_path: Some(appex_path.to_string_lossy().into_owned()),
        icon_path: None,
    }))
}

// =========================================================
// Display Name Resolution
// =========================================================

/// Resolve the localized display name for a settings extension.
fn resolve_display_name(appex_path: &Path, info: &plist::Dictionary, system_lang: &str) -> String {
    // Try the loctable first — it has properly localized names.
    let loctable_path = appex_path.join("Contents/Resources/InfoPlist.loctable");
    if let Some(name) = try_loctable_name(&loctable_path, system_lang) {
        return name;
    }

    // Fall back to the Info.plist fields.
    if let Some(name) = info.get("CFBundleDisplayName").and_then(|v| v.as_string()) {
        return name.to_string();
    }

    if let Some(name) = info.get("CFBundleName").and_then(|v| v.as_string()) {
        return name.to_string();
    }

    // Last resort: filename stem.
    appex_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| appex_path.display().to_string())
}

/// Try to read `CFBundleDisplayName` from an `InfoPlist.loctable`
/// binary plist. Tries the system language first, then falls back
/// to English.
fn try_loctable_name(loctable_path: &Path, system_lang: &str) -> Option<String> {
    let loctable: plist::Dictionary = plist::from_file(loctable_path).ok()?;

    // Try system language first.
    if let Some(name) = loctable_display_name(&loctable, system_lang) {
        return Some(name);
    }

    // Fall back to English.
    if system_lang != "en"
        && let Some(name) = loctable_display_name(&loctable, "en")
    {
        return Some(name);
    }

    None
}

/// Extract `CFBundleDisplayName` from a specific locale entry
/// in the loctable dictionary.
fn loctable_display_name(loctable: &plist::Dictionary, lang: &str) -> Option<String> {
    loctable
        .get(lang)
        .and_then(|v| v.as_dictionary())
        .and_then(|d| d.get("CFBundleDisplayName"))
        .and_then(|v| v.as_string())
        .map(String::from)
}

// =========================================================
// System Language Resolution
// =========================================================

/// Get the system's preferred language code (e.g., "de", "en",
/// "ja") via the Tauri OS plugin.
fn resolve_system_language() -> String {
    let locale = tauri_plugin_os::locale().unwrap_or_else(|| "en".to_string());

    // BCP-47 tags look like "de-DE" or "en-US". We only need
    // the language subtag for loctable lookup.
    locale.split('-').next().unwrap_or("en").to_string()
}
