// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Discovery (macOS)
//
// Discovers installed applications by querying the Spotlight
// index via the `mdfind` CLI and parsing each app bundle's
// `Info.plist` for display name and visibility metadata.
//
// The Mdfind struct encapsulates the raw subprocess call so
// the discovery logic stays focused on metadata extraction.
// =========================================================

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use image::DynamicImage;
use tauri_plugin_opener::OpenerExt;

use super::cgimage_conversion::nsworkspace_icon_for_file;
use crate::platform::app_discovery::{AppDiscovery, DiscoveredApp};

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

    fn icon(&self, app: &DiscoveredApp) -> anyhow::Result<Option<DynamicImage>> {
        nsworkspace_icon_for_file(&app.path.to_string_lossy())
    }

    fn open(&self, entry_id: &str, app: &tauri::AppHandle) -> anyhow::Result<()> {
        app.opener()
            .open_path(entry_id, None::<&str>)
            .context("open application")
    }

    fn reveal(&self, entry_id: &str, app: &tauri::AppHandle) -> anyhow::Result<()> {
        app.opener()
            .reveal_item_in_dir(entry_id)
            .context("reveal application in file manager")
    }
}
