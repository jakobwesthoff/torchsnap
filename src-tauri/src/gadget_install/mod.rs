// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget Install / Uninstall
//
// Backend for the Gadgets settings panel's install and
// uninstall flows. Both return `requires_restart: true`
// because the `GadgetHost` slot list is frozen after setup;
// a hot lifecycle path is tracked in
// `todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`.
//
// The logic works on plain values that `setup` manages as
// Tauri state: `InstallPaths` for the filesystem layout,
// `RegisteredGadgets` for the frozen registry, and a
// `SettingsKeys` handle for the settings store. The Tauri
// commands only unpack that state and move the work onto a
// blocking thread.
//
// Install steps:
//
// 1. Open the source `.torchsnap` via `ArchiveSource::open`,
//    which validates the zip and parses the manifest. The
//    path guard that runs during `Manifest::parse` catches
//    traversal here.
// 2. Reject if a gadget with the manifest's id is already
//    registered with any `GadgetSourceKind`, with a message
//    specific to the colliding kind.
// 3. Publish the archive as `gadgets/<id>.torchsnap`
//    (`archive_ops::publish_fresh`).
//
// Uninstall steps:
//
// 1. Reject unless the id is registered as
//    `GadgetSourceKind::User`; built-in, system, and dev
//    gadgets are not uninstallable through this flow.
// 2. Remove the archive, the directory form if any, and the
//    state tree (`archive_ops::remove_user_gadget`).
// 3. Strip the gadget's settings keys and persist the store
//    (`store::strip_gadget_settings`).
// =========================================================

mod archive_ops;
mod paths;
mod registered;
mod store;

pub use paths::InstallPaths;
pub use registered::RegisteredGadgets;
pub use store::SettingsKeys;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use serde::Serialize;

use crate::wasm::source::{ArchiveSource, GadgetSource, GadgetSourceKind};

// =========================================================
// Response types
// =========================================================

/// Metadata returned after a successful install. The
/// frontend renders `name` + `version` in a confirmation
/// banner and uses `requires_restart` to decide whether to
/// show the restart prompt.
#[derive(Debug, Serialize)]
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
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub requires_restart: bool,
}

// =========================================================
// Tauri commands
// =========================================================

/// Install a user-supplied `.torchsnap` archive into the
/// app data gadgets directory. Returns an error string that
/// the frontend can surface directly in a toast or banner.
#[tauri::command]
pub async fn install_gadget_archive(
    paths: tauri::State<'_, InstallPaths>,
    registered: tauri::State<'_, RegisteredGadgets>,
    archive_path: String,
) -> Result<InstalledGadgetInfo, String> {
    let paths = paths.inner().clone();
    let registered = registered.inner().clone();
    let archive_path = PathBuf::from(archive_path);
    tokio::task::spawn_blocking(move || install(&paths, &registered, &archive_path))
        .await
        .map_err(|e| format!("install task panicked: {e}"))?
        .map_err(|e| format!("{e:#}"))
}

/// Uninstall a user-installed gadget. Rejects built-in,
/// system, and dev gadgets.
#[tauri::command]
pub async fn uninstall_user_gadget(
    paths: tauri::State<'_, InstallPaths>,
    registered: tauri::State<'_, RegisteredGadgets>,
    settings: tauri::State<'_, Arc<dyn SettingsKeys>>,
    gadget_id: String,
) -> Result<UninstallResult, String> {
    let paths = paths.inner().clone();
    let registered = registered.inner().clone();
    let settings = Arc::clone(settings.inner());
    tokio::task::spawn_blocking(move || {
        uninstall(&paths, &registered, settings.as_ref(), &gadget_id)
    })
    .await
    .map_err(|e| format!("uninstall task panicked: {e}"))?
    .map_err(|e| format!("{e:#}"))
}

// =========================================================
// Install
// =========================================================

fn install(
    paths: &InstallPaths,
    registered: &RegisteredGadgets,
    archive_path: &Path,
) -> anyhow::Result<InstalledGadgetInfo> {
    let source =
        ArchiveSource::open(archive_path).context("open gadget archive for installation")?;
    let manifest = source.manifest().clone();
    let gadget_id = manifest.gadget.id.as_str().to_string();

    // Each kind gets its own message so the user can tell which
    // rejection applies and what remediation, if any, fits.
    if let Some(kind) = registered.kind(&gadget_id) {
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

    // The archive handle has to be closed before the copy: on
    // Windows an open file blocks the later rename.
    drop(source);
    archive_ops::publish_fresh(paths, archive_path, &gadget_id)?;

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

fn uninstall(
    paths: &InstallPaths,
    registered: &RegisteredGadgets,
    settings: &dyn SettingsKeys,
    gadget_id: &str,
) -> anyhow::Result<UninstallResult> {
    let kind = registered
        .kind(gadget_id)
        .ok_or_else(|| anyhow::anyhow!("unknown gadget id `{gadget_id}`"))?;
    if kind != GadgetSourceKind::User {
        anyhow::bail!(
            "gadget `{gadget_id}` is a {kind:?} gadget; only user-installed gadgets can be uninstalled"
        );
    }

    let removal = archive_ops::remove_user_gadget(paths, gadget_id)?;
    // A registered user gadget without `<id>.torchsnap` means the
    // file on disk does not match its manifest id (a renamed or
    // hand-placed archive). Its state is still removed, but the
    // stray archive would load again after restart, so this is
    // worth a trace.
    if !removal.archive_removed && !removal.directory_removed {
        eprintln!(
            "uninstall of `{gadget_id}` found neither `{}` nor `{}`",
            paths.archive(gadget_id).display(),
            paths.directory(gadget_id).display()
        );
    }

    store::strip_gadget_settings(settings, gadget_id)?;

    Ok(UninstallResult {
        requires_restart: true,
    })
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::gadget_install::store::MemorySettings;
    use crate::wasm::manifest::test_helpers::{
        archive_with_manifest, write_archive_without_manifest,
    };

    fn temp_paths() -> (tempfile::TempDir, InstallPaths) {
        let root = tempfile::tempdir().expect("create temp app data dir");
        let paths = InstallPaths::new(root.path());
        (root, paths)
    }

    fn registered(entries: &[(&str, GadgetSourceKind)]) -> RegisteredGadgets {
        RegisteredGadgets::from_kinds(
            entries
                .iter()
                .map(|(id, kind)| (id.to_string(), *kind))
                .collect::<HashMap<_, _>>(),
        )
    }

    fn dir_entries(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .map(|entry| {
                        entry
                            .expect("read dir entry")
                            .file_name()
                            .to_string_lossy()
                            .into_owned()
                    })
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    // =========================================================
    // Install
    // =========================================================

    #[test]
    fn install_publishes_the_archive_under_its_manifest_id() {
        let (_root, paths) = temp_paths();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");

        let info = install(&paths, &registered(&[]), &archive).expect("install should succeed");

        assert_eq!(info.id, "weather");
        assert_eq!(info.version, "1.4.0");
        assert!(info.requires_restart);
        assert_eq!(dir_entries(&paths.gadgets_dir), vec!["weather.torchsnap"]);
        assert_eq!(
            std::fs::read(paths.archive("weather")).expect("published archive"),
            std::fs::read(&archive).expect("source archive")
        );
    }

    #[test]
    fn install_rejects_ids_owned_by_other_source_kinds() {
        let cases = [
            (
                GadgetSourceKind::Builtin,
                "A built-in gadget with id `weather` already exists",
            ),
            (
                GadgetSourceKind::System,
                "A system gadget with id `weather` is bundled with the app",
            ),
            (
                GadgetSourceKind::Dev,
                "A development gadget with id `weather` is loaded from the repository",
            ),
            (
                GadgetSourceKind::User,
                "A user gadget with id `weather` is already installed",
            ),
        ];
        for (kind, expected) in cases {
            let (_root, paths) = temp_paths();
            let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");

            let error = install(&paths, &registered(&[("weather", kind)]), &archive)
                .expect_err("install over an existing id should fail");

            assert!(
                format!("{error:#}").contains(expected),
                "{kind:?}: unexpected message `{error:#}`"
            );
            assert!(dir_entries(&paths.gadgets_dir).is_empty());
        }
    }

    #[test]
    fn install_rejects_a_file_that_is_not_a_zip() {
        let (_root, paths) = temp_paths();
        let source = tempfile::tempdir().expect("create source dir");
        let not_a_zip = source.path().join("weather.torchsnap");
        std::fs::write(&not_a_zip, b"definitely not a zip").expect("write source file");

        assert!(install(&paths, &registered(&[]), &not_a_zip).is_err());
        assert!(dir_entries(&paths.gadgets_dir).is_empty());
    }

    #[test]
    fn install_rejects_an_archive_without_manifest() {
        let (_root, paths) = temp_paths();
        let (_src, archive) = write_archive_without_manifest(&[("README.md", b"no manifest here")]);

        assert!(install(&paths, &registered(&[]), &archive).is_err());
        assert!(dir_entries(&paths.gadgets_dir).is_empty());
    }

    // =========================================================
    // Uninstall
    // =========================================================

    fn install_user_gadget_on_disk(paths: &InstallPaths, id: &str) {
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive(id), b"archive").expect("write archive");
        std::fs::create_dir_all(paths.home(id).join("compile-cache")).expect("create gadget home");
        std::fs::write(paths.home(id).join("storage.sqlite3"), b"db").expect("write gadget data");
    }

    #[test]
    fn uninstall_removes_the_archive_the_directory_form_and_the_home_tree() {
        let (_root, paths) = temp_paths();
        install_user_gadget_on_disk(&paths, "weather");
        std::fs::create_dir_all(paths.directory("weather")).expect("create directory form");
        let settings = MemorySettings::with_keys(&["enabled.weather", "gadgets.weather.city"]);

        let result = uninstall(
            &paths,
            &registered(&[("weather", GadgetSourceKind::User)]),
            &settings,
            "weather",
        )
        .expect("uninstall should succeed");

        assert!(result.requires_restart);
        assert!(dir_entries(&paths.gadgets_dir).is_empty());
        assert!(!paths.home("weather").exists());
        assert!(settings.keys().is_empty());
        assert_eq!(settings.save_count(), 1);
    }

    #[test]
    fn uninstall_rejects_unknown_ids_and_non_user_gadgets() {
        let (_root, paths) = temp_paths();
        let settings = MemorySettings::with_keys(&[]);
        let registry = registered(&[("clipboard-manager", GadgetSourceKind::Builtin)]);

        let unknown = uninstall(&paths, &registry, &settings, "weather").expect_err("unknown id");
        assert!(format!("{unknown:#}").contains("unknown gadget id `weather`"));

        let builtin =
            uninstall(&paths, &registry, &settings, "clipboard-manager").expect_err("builtin");
        assert!(format!("{builtin:#}").contains("only user-installed gadgets can be uninstalled"));
        assert_eq!(settings.save_count(), 0);
    }

    #[test]
    fn uninstall_propagates_a_failed_settings_save() {
        let (_root, paths) = temp_paths();
        install_user_gadget_on_disk(&paths, "weather");
        let settings = MemorySettings::with_keys(&["enabled.weather"]).failing_save();

        let error = uninstall(
            &paths,
            &registered(&[("weather", GadgetSourceKind::User)]),
            &settings,
            "weather",
        )
        .expect_err("a failed save must not be reported as success");

        assert!(format!("{error:#}").contains("persist settings"));
    }
}
