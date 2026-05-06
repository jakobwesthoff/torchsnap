// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget Install / Uninstall
//
// Backend for the Gadgets settings panel's install and
// uninstall flows. Both commands return
// `requires_restart: true` because the `GadgetHost` slot
// list is frozen after setup — a hot lifecycle path is
// tracked in `todos/wasm/…-gadget-hot-lifecycle.md`.
//
// Install steps:
//
// 1. Open the source `.torchsnap` via `ArchiveSource::open`,
//    which validates the zip and parses the manifest. The
//    path guard that runs during `Manifest::parse` catches
//    traversal here.
// 2. Extract the manifest's gadget id.
// 3. Reject if a gadget with that id is already registered
//    with any `GadgetSourceKind`, with a message specific to
//    the colliding kind.
// 4. Atomically copy the archive into
//    `<app_data_dir>/gadgets/<id>.torchsnap` via a
//    temp-then-rename dance so a mid-copy crash leaves no
//    partial archive behind.
//
// Uninstall steps:
//
// 1. Look up the gadget's source kind. Reject unless it is
//    `GadgetSourceKind::User`; built-in, system, and dev
//    gadgets are not uninstallable through this flow.
// 2. Remove the archive at `<app_data_dir>/gadgets/<id>.torchsnap`,
//    the unpacked dir at `<app_data_dir>/gadgets/<id>/` if
//    any (dev-style user gadget), and the state tree at
//    `<app_data_dir>/gadget-home/<id>/`.
// 3. Strip settings keys — `enabled.<id>` and every
//    `gadgets.<id>.*` key. The exact-match-plus-prefix
//    design keeps unrelated gadgets' settings intact even
//    when gadget ids share a textual prefix (e.g. `calc` vs
//    `calculator`).
// =========================================================

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;

use crate::gadget_host::GadgetHost;
use crate::wasm::source::{ArchiveSource, GadgetSource, GadgetSourceKind};

// =========================================================
// Response types
// =========================================================

/// Metadata returned after a successful install. The
/// frontend renders `name` + `version` in a confirmation
/// banner and uses `requires_restart` to decide whether to
/// show the restart prompt.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledGadgetInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub requires_restart: bool,
}

/// Minimal result for uninstall. Kept as a struct (not a
/// bare `bool`) so future fields (e.g. a list of cleaned-up
/// paths for the UI to surface) can be added without
/// breaking the IPC contract.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub requires_restart: bool,
}

// =========================================================
// Install
// =========================================================

/// Install a user-supplied `.torchsnap` archive into the
/// app data gadgets directory. Returns an error string that
/// the frontend can surface directly in a toast or banner.
///
/// Runs the blocking filesystem and zip work on a
/// `spawn_blocking` thread so the Tauri IPC thread is not
/// held up.
#[tauri::command]
pub async fn install_gadget_archive(
    app: AppHandle,
    host: tauri::State<'_, Arc<GadgetHost>>,
    archive_path: String,
) -> Result<InstalledGadgetInfo, String> {
    let host = Arc::clone(host.inner());
    let archive_path = PathBuf::from(archive_path);
    tokio::task::spawn_blocking(move || install_impl(&app, &host, &archive_path))
        .await
        .map_err(|e| format!("install task panicked: {e}"))?
        .map_err(|e| format!("{e:#}"))
}

fn install_impl(
    app: &AppHandle,
    host: &GadgetHost,
    archive_path: &Path,
) -> anyhow::Result<InstalledGadgetInfo> {
    // Opening the archive validates the zip structure, parses
    // the manifest, and runs the path guard on every
    // manifest-referenced file. If any of those fail the
    // archive is not a safe install candidate.
    let source =
        ArchiveSource::open(archive_path).context("open gadget archive for installation")?;
    let manifest = source.manifest().clone();
    let gadget_id = manifest.gadget.id.as_str().to_string();

    // Collision check against every already-registered source
    // kind. Each kind gets a distinct error message so the user
    // can tell which rejection applies and what remediation (if
    // any) fits their case.
    if let Some(kind) = host.gadget_sources().get(&gadget_id).copied() {
        match kind {
            GadgetSourceKind::Builtin => anyhow::bail!(
                "A built-in gadget with id `{gadget_id}` already exists. Built-in gadgets cannot be replaced."
            ),
            GadgetSourceKind::System => anyhow::bail!(
                "A system gadget with id `{gadget_id}` is bundled with the app. Overriding system gadgets is not supported."
            ),
            GadgetSourceKind::Dev => anyhow::bail!(
                "A development gadget with id `{gadget_id}` is loaded from the repository. Edit the dev gadget directly or change its id before installing."
            ),
            GadgetSourceKind::User => anyhow::bail!(
                "A user gadget with id `{gadget_id}` is already installed. Uninstall the existing version, then retry."
            ),
        }
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .context("resolve app data dir for gadget install")?;
    let gadgets_dir = app_data_dir.join("gadgets");
    std::fs::create_dir_all(&gadgets_dir).context("create gadgets directory")?;

    let final_path = gadgets_dir.join(format!("{gadget_id}.torchsnap"));

    // Drop the source handle before the copy/rename — on
    // Windows the archive file would otherwise still be open
    // and the rename could fail. On macOS/Linux this is a
    // defensive cleanup.
    drop(source);

    // Atomic publish: copy to a dot-prefixed temp in the same
    // directory (same filesystem guarantees atomic rename),
    // then rename into place. A mid-copy crash leaves a file
    // the directory scan skips (it is not a plain
    // `*.torchsnap` entry); a crash after rename leaves a
    // valid install.
    let tmp_path = gadgets_dir.join(format!(".{gadget_id}.torchsnap.tmp"));
    std::fs::copy(archive_path, &tmp_path).context("copy archive into staging location")?;
    std::fs::rename(&tmp_path, &final_path).context("publish staged archive")?;

    Ok(InstalledGadgetInfo {
        id: gadget_id,
        name: manifest.gadget.name,
        version: manifest.gadget.version,
        requires_restart: true,
    })
}

// =========================================================
// Uninstall
// =========================================================

/// Uninstall a user-installed gadget. Rejects built-in,
/// system, and dev gadgets. Removes the archive, the
/// optional unpacked dir, the gadget-home state tree, and
/// the gadget's settings keys.
#[tauri::command]
pub async fn uninstall_user_gadget(
    app: AppHandle,
    host: tauri::State<'_, Arc<GadgetHost>>,
    gadget_id: String,
) -> Result<UninstallResult, String> {
    let host = Arc::clone(host.inner());
    tokio::task::spawn_blocking(move || uninstall_impl(&app, &host, &gadget_id))
        .await
        .map_err(|e| format!("uninstall task panicked: {e}"))?
        .map_err(|e| format!("{e:#}"))
}

fn uninstall_impl(
    app: &AppHandle,
    host: &GadgetHost,
    gadget_id: &str,
) -> anyhow::Result<UninstallResult> {
    let kind = host
        .gadget_sources()
        .get(gadget_id)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("unknown gadget id `{gadget_id}`"))?;

    if kind != GadgetSourceKind::User {
        anyhow::bail!(
            "gadget `{gadget_id}` is a {kind:?} gadget — only user-installed gadgets can be uninstalled"
        );
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .context("resolve app data dir for gadget uninstall")?;

    // Archive form: `<app_data_dir>/gadgets/<id>.torchsnap`.
    let archive = app_data_dir
        .join("gadgets")
        .join(format!("{gadget_id}.torchsnap"));
    if archive.exists() {
        std::fs::remove_file(&archive).context("remove gadget archive")?;
    }

    // Directory form: `<app_data_dir>/gadgets/<id>/`. A
    // dev-style user gadget may be shipped this way by a gadget
    // author testing a release flow.
    let dir = app_data_dir.join("gadgets").join(gadget_id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).context("remove gadget directory")?;
    }

    // State tree: `<app_data_dir>/gadget-home/<id>/`. Dropped
    // unconditionally; the gadget can never reach it after the
    // restart the caller is about to perform.
    let home = app_data_dir.join("gadget-home").join(gadget_id);
    if home.exists() {
        std::fs::remove_dir_all(&home).context("remove gadget-home directory")?;
    }

    // Strip the gadget's settings keys. The store API has no
    // bulk delete, so we snapshot the matching keys first and
    // delete them by name. A `gadgets.<id>.` prefix test plus
    // the exact `enabled.<id>` key together cover every key
    // the host writes for a gadget; unrelated gadgets whose id
    // shares a text prefix (e.g. `calc` vs `calculator`) are
    // left alone because `gadgets.calc.` does not match
    // `gadgets.calculator.foo`.
    let store = app
        .store("settings.json")
        .context("open settings store for uninstall cleanup")?;
    let enabled_key = format!("enabled.{gadget_id}");
    let prefix = format!("gadgets.{gadget_id}.");
    let mut to_delete: Vec<String> = Vec::new();
    for (key, _) in store.entries() {
        if key == enabled_key || key.starts_with(&prefix) {
            to_delete.push(key);
        }
    }
    for key in to_delete {
        store.delete(&key);
    }
    let _ = store.save();

    Ok(UninstallResult {
        requires_restart: true,
    })
}

// =========================================================
// Tests
// =========================================================
//
// Unit tests focus on error paths that do not require a full
// Tauri `AppHandle`: settings-key cleanup, collision
// rejection logic, manifest parsing failure. The happy path
// (atomic copy + spawn_blocking) is exercised end-to-end by
// the manual E2E checklist in the plan's verification
// section.

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use super::*;

    // =========================================================
    // settings-key cleanup regression coverage
    //
    // A gadget id that shares a textual prefix with another
    // must not strip the other's keys. The host writes only
    // `enabled.<id>` and `gadgets.<id>.*`, so these two
    // patterns are the authoritative matchers. A bug using
    // `starts_with("enabled.")` or a naive prefix-only match
    // would silently nuke unrelated state.
    // =========================================================

    /// Mimic the key-matching logic inside `uninstall_impl`
    /// so the behaviour is testable without a real Store.
    /// Any future refactor that changes the matcher must
    /// keep these tests passing.
    fn keys_to_strip_for(gadget_id: &str, all_keys: &[&str]) -> Vec<String> {
        let enabled_key = format!("enabled.{gadget_id}");
        let prefix = format!("gadgets.{gadget_id}.");
        all_keys
            .iter()
            .filter(|k| **k == enabled_key || k.starts_with(&prefix))
            .map(|k| k.to_string())
            .collect()
    }

    #[test]
    fn cleanup_matches_exact_enabled_key() {
        let keys = ["enabled.foo", "enabled.foobar", "enabled.bar"];
        let stripped = keys_to_strip_for("foo", &keys);
        assert_eq!(stripped, vec!["enabled.foo".to_string()]);
    }

    #[test]
    fn cleanup_matches_gadget_settings_prefix() {
        let keys = [
            "gadgets.foo.alpha",
            "gadgets.foo.beta",
            "gadgets.foo.nested.key",
        ];
        let stripped = keys_to_strip_for("foo", &keys);
        assert_eq!(stripped.len(), 3);
    }

    /// Regression: a gadget id that is a textual prefix of
    /// another must not match the longer id's keys. Uninstall
    /// of `calc` would have otherwise stripped every
    /// `calculator.*` setting — a silent data-loss bug.
    #[test]
    fn cleanup_ignores_other_gadgets_with_longer_ids() {
        let keys = [
            "enabled.calc",
            "enabled.calculator",
            "gadgets.calc.shortcut",
            "gadgets.calculator.history",
            "gadgets.calculator.enabled",
        ];
        let stripped = keys_to_strip_for("calc", &keys);
        assert_eq!(stripped.len(), 2);
        assert!(stripped.contains(&"enabled.calc".to_string()));
        assert!(stripped.contains(&"gadgets.calc.shortcut".to_string()));
    }

    #[test]
    fn cleanup_leaves_unrelated_keys_intact() {
        let keys = [
            "appearance.theme",
            "frecency.enabled",
            "websiteMetadata.cacheTtlDays",
            "enabled.other-gadget",
            "gadgets.other-gadget.key",
        ];
        let stripped = keys_to_strip_for("foo", &keys);
        assert!(
            stripped.is_empty(),
            "no keys should be stripped for an absent gadget"
        );
    }

    #[test]
    fn cleanup_is_empty_when_no_matching_keys_exist() {
        let stripped = keys_to_strip_for("foo", &[]);
        assert!(stripped.is_empty());
    }

    // =========================================================
    // Install error paths that do not need an AppHandle
    // =========================================================

    /// `ArchiveSource::open` rejects a non-archive file. The
    /// install command surfaces this error to the UI as a
    /// "not a torchsnap archive" banner — pinning the failure
    /// mode here guards against a regression where a broken
    /// zip slips past and gets renamed into the gadgets dir.
    #[test]
    fn install_rejects_non_zip_source_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let not_a_zip = tmp.path().join("not-an-archive.torchsnap");
        std::fs::write(&not_a_zip, b"definitely not a zip").expect("write");

        let result = ArchiveSource::open(&not_a_zip);
        assert!(result.is_err());
    }

    /// A zip that exists but contains no `manifest.toml` is
    /// not a valid gadget archive — installing such a file
    /// would leave a registered entry with no manifest.
    #[test]
    fn install_rejects_zip_without_manifest() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let zip_path = tmp.path().join("empty.torchsnap");
        let file = std::fs::File::create(&zip_path).expect("create");
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file::<_, ()>("README.md", zip::write::SimpleFileOptions::default())
            .expect("start file");
        writer
            .write_all(b"this zip has no manifest.toml")
            .expect("write");
        writer.finish().expect("finish");

        let result = ArchiveSource::open(&zip_path);
        assert!(result.is_err());
    }
}
