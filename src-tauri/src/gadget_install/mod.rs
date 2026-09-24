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
// 2. Decide from the frozen registry and the changes made
//    since startup (`decision::decide_install`).
// 3. Publish the archive as `gadgets/<id>.torchsnap`, either
//    fresh or as a replace that keeps the current archive as
//    `.<id>.torchsnap.prev` for undo, and record it in
//    `PendingChanges`. Replacing never touches the gadget's
//    data or settings.
//
// Uninstall steps:
//
// 1. Decide the same way (`decision::decide_uninstall`):
//    user gadgets and installs made since startup can be
//    removed, built-in, system and dev gadgets cannot.
// 2. Remove the archive and the directory form if any, and
//    leave an uninstall marker (`archive_ops::remove_user_gadget`).
//    The gadget keeps running until restart, so its state tree
//    and settings are deleted by the next startup
//    (`archive_ops::process_uninstall_markers`), before any
//    gadget loads.
// 3. Record the uninstall in `PendingChanges`.
// =========================================================

mod archive_ops;
mod decision;
mod paths;
mod pending;
mod registered;
mod store;

pub use archive_ops::{process_uninstall_markers, remove_stale_backups};
pub use paths::InstallPaths;
pub use pending::PendingChanges;
pub use registered::RegisteredGadgets;
pub use store::SettingsKeys;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use serde::Serialize;

use archive_ops::Undone;
use decision::{
    InstallDecision, UninstallDecision, VersionRelation, decide_install, decide_uninstall,
};
use pending::PendingChange;

use crate::wasm::source::{ArchiveSource, GadgetSource};

// =========================================================
// Response types
// =========================================================

/// Metadata returned after a successful install. The
/// frontend renders `name` + `version` in a confirmation
/// banner and uses `requires_restart` to decide whether to
/// show the restart prompt. `previous_version` and
/// `version_relation` are set when the install replaced
/// another version of the gadget.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledGadgetInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub previous_version: Option<String>,
    pub version_relation: Option<VersionRelation>,
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

/// Result of undoing an install. `restored_version` is the version
/// back on disk after undoing a replace; `None` means the undone
/// install left no archive for the gadget.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub restored_version: Option<String>,
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
    pending: tauri::State<'_, Arc<Mutex<PendingChanges>>>,
    archive_path: String,
) -> Result<InstalledGadgetInfo, String> {
    let paths = paths.inner().clone();
    let registered = registered.inner().clone();
    let pending = Arc::clone(pending.inner());
    let archive_path = PathBuf::from(archive_path);
    tokio::task::spawn_blocking(move || {
        let mut pending = pending
            .lock()
            .expect("pending changes lock is never poisoned");
        install(&paths, &registered, &mut pending, &archive_path)
    })
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
    pending: tauri::State<'_, Arc<Mutex<PendingChanges>>>,
    gadget_id: String,
) -> Result<UninstallResult, String> {
    let paths = paths.inner().clone();
    let registered = registered.inner().clone();
    let pending = Arc::clone(pending.inner());
    tokio::task::spawn_blocking(move || {
        let mut pending = pending
            .lock()
            .expect("pending changes lock is never poisoned");
        uninstall(&paths, &registered, &mut pending, &gadget_id)
    })
    .await
    .map_err(|e| format!("uninstall task panicked: {e}"))?
    .map_err(|e| format!("{e:#}"))
}

/// Undo the install or replace of `gadget_id` made in this
/// session.
#[tauri::command]
pub async fn install_undo(
    paths: tauri::State<'_, InstallPaths>,
    registered: tauri::State<'_, RegisteredGadgets>,
    pending: tauri::State<'_, Arc<Mutex<PendingChanges>>>,
    gadget_id: String,
) -> Result<UndoResult, String> {
    let paths = paths.inner().clone();
    let registered = registered.inner().clone();
    let pending = Arc::clone(pending.inner());
    tokio::task::spawn_blocking(move || {
        let mut pending = pending
            .lock()
            .expect("pending changes lock is never poisoned");
        undo(&paths, &registered, &mut pending, &gadget_id)
    })
    .await
    .map_err(|e| format!("undo task panicked: {e}"))?
    .map_err(|e| format!("{e:#}"))
}

// =========================================================
// Install
// =========================================================

fn install(
    paths: &InstallPaths,
    registered: &RegisteredGadgets,
    pending: &mut PendingChanges,
    archive_path: &Path,
) -> anyhow::Result<InstalledGadgetInfo> {
    let source =
        ArchiveSource::open(archive_path).context("open gadget archive for installation")?;
    let manifest = source.manifest().clone();
    let gadget_id = manifest.gadget.id.as_str().to_string();

    let decision = decide_install(
        registered.get(&gadget_id),
        pending.get(&gadget_id),
        &manifest,
    );

    // The archive handle has to be closed before the copy: on
    // Windows an open file blocks the later rename.
    drop(source);

    let (previous_version, version_relation) = match decision {
        InstallDecision::Reject(message) => anyhow::bail!(message),
        InstallDecision::Fresh => {
            archive_ops::publish_fresh(paths, archive_path, &gadget_id)?;
            pending.record_install(&gadget_id, manifest.clone());
            (None, None)
        }
        InstallDecision::Replace { previous, relation } => {
            archive_ops::publish_replace(paths, archive_path, &gadget_id)?;
            pending.record_replace(&gadget_id, &previous.gadget.version, manifest.clone());
            (Some(previous.gadget.version), Some(relation))
        }
    };

    Ok(InstalledGadgetInfo {
        id: gadget_id,
        name: manifest.gadget.name,
        version: manifest.gadget.version,
        previous_version,
        version_relation,
        requires_restart: true,
    })
}

// =========================================================
// Undo
// =========================================================

/// Reverse the install or replace of `gadget_id` made in this
/// session. The backup written by the first replace decides what
/// comes back; without one, the install was fresh and its archive
/// is removed.
fn undo(
    paths: &InstallPaths,
    registered: &RegisteredGadgets,
    pending: &mut PendingChanges,
    gadget_id: &str,
) -> anyhow::Result<UndoResult> {
    let change = match pending.get(gadget_id) {
        Some(change @ (PendingChange::Installed { .. } | PendingChange::Replaced { .. })) => {
            change.clone()
        }
        None | Some(PendingChange::Uninstalled) => {
            anyhow::bail!("nothing to undo for `{gadget_id}` in this session")
        }
    };

    let restored_version = match (archive_ops::undo_publish(paths, gadget_id)?, change) {
        // The backup is the archive that loaded at startup, so the
        // startup state is back and nothing is pending any more.
        (Undone::Restored, PendingChange::Replaced { .. }) => {
            pending.forget(gadget_id);
            Some(manifest_on_disk(paths, gadget_id)?.gadget.version)
        }
        // The backup is the first install of this session.
        (Undone::Restored, _) => {
            let manifest = manifest_on_disk(paths, gadget_id)?;
            let version = manifest.gadget.version.clone();
            pending.record_install(gadget_id, manifest);
            Some(version)
        }
        // A fresh install is gone again. A registered gadget that was
        // uninstalled before it keeps its uninstall marker, so it is
        // back to plain "uninstalled".
        (Undone::Removed, _) => {
            pending.record_uninstall(gadget_id, registered.get(gadget_id).is_some());
            None
        }
    };

    Ok(UndoResult {
        restored_version,
        requires_restart: true,
    })
}

fn manifest_on_disk(
    paths: &InstallPaths,
    gadget_id: &str,
) -> anyhow::Result<crate::wasm::manifest::Manifest> {
    Ok(ArchiveSource::open(paths.archive(gadget_id))
        .context("read the restored archive")?
        .manifest()
        .clone())
}

// =========================================================
// Uninstall
// =========================================================

fn uninstall(
    paths: &InstallPaths,
    registered: &RegisteredGadgets,
    pending: &mut PendingChanges,
    gadget_id: &str,
) -> anyhow::Result<UninstallResult> {
    let registration = registered.get(gadget_id);
    match decide_uninstall(registration, pending.get(gadget_id), gadget_id) {
        UninstallDecision::Allowed => {}
        UninstallDecision::Reject(message) => anyhow::bail!(message),
    }

    let removal = archive_ops::remove_user_gadget(paths, gadget_id)?;
    // A registered user gadget without `<id>.torchsnap` means the
    // file on disk does not match its manifest id (a renamed or
    // hand-placed archive). Its state is still cleaned up, but the
    // stray archive would load again after restart, so this is
    // worth a trace.
    if !removal.archive_removed && !removal.directory_removed {
        eprintln!(
            "uninstall of `{gadget_id}` found neither `{}` nor `{}`",
            paths.archive(gadget_id).display(),
            paths.directory(gadget_id).display()
        );
    }

    pending.record_uninstall(gadget_id, registration.is_some());

    Ok(UninstallResult {
        requires_restart: true,
    })
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gadget_install::registered::Registration;
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::{
        archive_with_manifest, write_archive_without_manifest,
    };

    fn temp_paths() -> (tempfile::TempDir, InstallPaths) {
        let root = tempfile::tempdir().expect("create temp app data dir");
        let paths = InstallPaths::new(root.path());
        (root, paths)
    }

    fn manifest_of(archive: &Path) -> Manifest {
        ArchiveSource::open(archive)
            .expect("fixture archive opens")
            .manifest()
            .clone()
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

    /// Place a user gadget on disk the way a previous session left it,
    /// with some data in its home tree, and return its registration.
    fn registered_user_gadget(paths: &InstallPaths, id: &str, version: &str) -> RegisteredGadgets {
        let (_src, archive) = archive_with_manifest(id, version, "");
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::copy(&archive, paths.archive(id)).expect("place installed archive");
        std::fs::create_dir_all(paths.home(id)).expect("create gadget home");
        std::fs::write(paths.home(id).join("storage.sqlite3"), b"db").expect("write gadget data");
        RegisteredGadgets::from_entries([(
            id.to_string(),
            Registration::User {
                manifest: Box::new(manifest_of(&archive)),
                is_directory: false,
            },
        )])
    }

    // =========================================================
    // Fresh install
    // =========================================================

    #[test]
    fn install_publishes_the_archive_under_its_manifest_id() {
        let (_root, paths) = temp_paths();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");

        let info = install(
            &paths,
            &RegisteredGadgets::default(),
            &mut PendingChanges::default(),
            &archive,
        )
        .expect("install should succeed");

        assert_eq!(info.id, "weather");
        assert_eq!(info.version, "1.4.0");
        assert_eq!(info.previous_version, None);
        assert_eq!(info.version_relation, None);
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
                Registration::Builtin,
                "A built-in gadget with id `weather` already exists",
            ),
            (
                Registration::System,
                "A system gadget with id `weather` is bundled with the app",
            ),
            (
                Registration::Dev,
                "A development gadget with id `weather` is loaded from the repository",
            ),
        ];
        for (registration, expected) in cases {
            let (_root, paths) = temp_paths();
            let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");
            let registry = RegisteredGadgets::from_entries([("weather".to_string(), registration)]);

            let error = install(&paths, &registry, &mut PendingChanges::default(), &archive)
                .expect_err("install over an existing id should fail");

            assert!(format!("{error:#}").contains(expected), "`{error:#}`");
            assert!(dir_entries(&paths.gadgets_dir).is_empty());
        }
    }

    #[test]
    fn install_rejects_a_file_that_is_not_a_zip() {
        let (_root, paths) = temp_paths();
        let source = tempfile::tempdir().expect("create source dir");
        let not_a_zip = source.path().join("weather.torchsnap");
        std::fs::write(&not_a_zip, b"definitely not a zip").expect("write source file");

        let result = install(
            &paths,
            &RegisteredGadgets::default(),
            &mut PendingChanges::default(),
            &not_a_zip,
        );

        assert!(result.is_err());
        assert!(dir_entries(&paths.gadgets_dir).is_empty());
    }

    #[test]
    fn install_rejects_an_archive_without_manifest() {
        let (_root, paths) = temp_paths();
        let (_src, archive) = write_archive_without_manifest(&[("README.md", b"no manifest here")]);

        let result = install(
            &paths,
            &RegisteredGadgets::default(),
            &mut PendingChanges::default(),
            &archive,
        );

        assert!(result.is_err());
        assert!(dir_entries(&paths.gadgets_dir).is_empty());
    }

    // =========================================================
    // Replace
    // =========================================================

    #[test]
    fn installing_another_version_replaces_and_keeps_the_data() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        let (_src, v2) = archive_with_manifest("weather", "2.0.0", "");

        let info = install(&paths, &registry, &mut PendingChanges::default(), &v2)
            .expect("replace should succeed");

        assert_eq!(info.version, "2.0.0");
        assert_eq!(info.previous_version.as_deref(), Some("1.0.0"));
        assert_eq!(info.version_relation, Some(VersionRelation::Upgrade));
        assert_eq!(
            manifest_of(&paths.archive("weather")).gadget.version,
            "2.0.0"
        );
        assert!(paths.home("weather").join("storage.sqlite3").exists());
    }

    #[test]
    fn a_pending_install_can_be_replaced_before_restart() {
        let (_root, paths) = temp_paths();
        let registry = RegisteredGadgets::default();
        let mut pending = PendingChanges::default();
        let (_a, v1) = archive_with_manifest("weather", "1.0.0", "");
        let (_b, v2) = archive_with_manifest("weather", "1.1.0", "");
        install(&paths, &registry, &mut pending, &v1).expect("fresh install");

        let info =
            install(&paths, &registry, &mut pending, &v2).expect("replace of pending install");

        assert_eq!(info.previous_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            manifest_of(&paths.archive("weather")).gadget.version,
            "1.1.0"
        );
    }

    #[test]
    fn replace_is_rejected_for_a_gadget_installed_as_a_directory() {
        let (_root, paths) = temp_paths();
        let (_src, v1) = archive_with_manifest("weather", "1.0.0", "");
        std::fs::create_dir_all(paths.directory("weather")).expect("create directory form");
        let registry = RegisteredGadgets::from_entries([(
            "weather".to_string(),
            Registration::User {
                manifest: Box::new(manifest_of(&v1)),
                is_directory: true,
            },
        )]);
        let (_b, v2) = archive_with_manifest("weather", "2.0.0", "");

        let error = install(&paths, &registry, &mut PendingChanges::default(), &v2)
            .expect_err("replace over a directory form should fail");

        assert!(format!("{error:#}").contains("installed as a directory"));
        assert!(!paths.archive("weather").exists());
    }

    // =========================================================
    // Undo
    // =========================================================

    #[test]
    fn undo_of_a_replace_restores_the_running_version() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        let mut pending = PendingChanges::default();
        let (_src, v2) = archive_with_manifest("weather", "2.0.0", "");
        install(&paths, &registry, &mut pending, &v2).expect("replace");

        let undone = undo(&paths, &registry, &mut pending, "weather").expect("undo should succeed");

        assert_eq!(undone.restored_version.as_deref(), Some("1.0.0"));
        assert_eq!(
            manifest_of(&paths.archive("weather")).gadget.version,
            "1.0.0"
        );
        assert!(pending.get("weather").is_none());
    }

    #[test]
    fn undo_of_a_fresh_install_removes_it() {
        let (_root, paths) = temp_paths();
        let registry = RegisteredGadgets::default();
        let mut pending = PendingChanges::default();
        let (_src, v1) = archive_with_manifest("weather", "1.0.0", "");
        install(&paths, &registry, &mut pending, &v1).expect("fresh install");

        let undone = undo(&paths, &registry, &mut pending, "weather").expect("undo should succeed");

        assert_eq!(undone.restored_version, None);
        assert!(!paths.archive("weather").exists());
        assert!(pending.get("weather").is_none());
    }

    #[test]
    fn undo_of_a_replaced_pending_install_restores_the_first_install() {
        let (_root, paths) = temp_paths();
        let registry = RegisteredGadgets::default();
        let mut pending = PendingChanges::default();
        let (_a, v1) = archive_with_manifest("weather", "1.0.0", "");
        let (_b, v2) = archive_with_manifest("weather", "1.1.0", "");
        install(&paths, &registry, &mut pending, &v1).expect("fresh install");
        install(&paths, &registry, &mut pending, &v2).expect("replace");

        let undone = undo(&paths, &registry, &mut pending, "weather").expect("undo should succeed");

        assert_eq!(undone.restored_version.as_deref(), Some("1.0.0"));
        assert!(matches!(
            pending.get("weather").and_then(pending::PendingChange::manifest),
            Some(m) if m.gadget.version == "1.0.0"
        ));
    }

    #[test]
    fn undo_of_a_reinstall_after_uninstall_returns_to_uninstalled() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        let mut pending = PendingChanges::default();
        uninstall(&paths, &registry, &mut pending, "weather").expect("uninstall");
        let (_src, v2) = archive_with_manifest("weather", "2.0.0", "");
        install(&paths, &registry, &mut pending, &v2).expect("reinstall");

        undo(&paths, &registry, &mut pending, "weather").expect("undo should succeed");

        assert!(!paths.archive("weather").exists());
        assert!(paths.uninstall_marker("weather").exists());
        assert!(matches!(
            pending.get("weather"),
            Some(pending::PendingChange::Uninstalled)
        ));
    }

    #[test]
    fn undo_without_a_change_in_this_session_is_rejected() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");

        let error = undo(&paths, &registry, &mut PendingChanges::default(), "weather")
            .expect_err("nothing to undo");

        assert!(format!("{error:#}").contains("nothing to undo"));
    }

    // =========================================================
    // Uninstall
    // =========================================================

    #[test]
    fn uninstall_removes_both_forms_and_defers_the_state_cleanup() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        std::fs::create_dir_all(paths.directory("weather")).expect("create directory form");

        let result = uninstall(&paths, &registry, &mut PendingChanges::default(), "weather")
            .expect("uninstall should succeed");

        assert!(result.requires_restart);
        assert_eq!(dir_entries(&paths.gadgets_dir), vec![".weather.uninstall"]);
        assert!(paths.home("weather").join("storage.sqlite3").exists());
    }

    #[test]
    fn uninstall_rejects_unknown_ids_and_non_user_gadgets() {
        let (_root, paths) = temp_paths();
        let registry = RegisteredGadgets::from_entries([(
            "clipboard-manager".to_string(),
            Registration::Builtin,
        )]);

        let unknown = uninstall(&paths, &registry, &mut PendingChanges::default(), "weather")
            .expect_err("unknown id");
        assert!(format!("{unknown:#}").contains("unknown gadget id `weather`"));

        let builtin = uninstall(
            &paths,
            &registry,
            &mut PendingChanges::default(),
            "clipboard-manager",
        )
        .expect_err("builtin");
        assert!(format!("{builtin:#}").contains("only user-installed gadgets can be uninstalled"));
    }

    // =========================================================
    // Changes before restart
    // =========================================================

    #[test]
    fn a_gadget_installed_in_this_session_can_be_uninstalled_again() {
        let (_root, paths) = temp_paths();
        let registry = RegisteredGadgets::default();
        let mut pending = PendingChanges::default();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");
        install(&paths, &registry, &mut pending, &archive).expect("install should succeed");

        uninstall(&paths, &registry, &mut pending, "weather")
            .expect("uninstall of a pending install should succeed");

        assert!(!paths.archive("weather").exists());
        assert!(pending.get("weather").is_none());
        // With the record gone, the same file installs fresh again.
        let info =
            install(&paths, &registry, &mut pending, &archive).expect("reinstall should succeed");
        assert_eq!(info.previous_version, None);
    }

    #[test]
    fn an_uninstalled_gadget_can_be_reinstalled_before_restart() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        let mut pending = PendingChanges::default();
        uninstall(&paths, &registry, &mut pending, "weather").expect("uninstall should succeed");
        let (_src, archive) = archive_with_manifest("weather", "1.5.0", "");

        let info = install(&paths, &registry, &mut pending, &archive)
            .expect("reinstall after uninstall should succeed");

        assert_eq!(info.version, "1.5.0");
        assert_eq!(info.previous_version, None);
        assert!(paths.archive("weather").exists());
    }

    #[test]
    fn a_second_uninstall_before_restart_asks_for_a_restart() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        let mut pending = PendingChanges::default();
        uninstall(&paths, &registry, &mut pending, "weather").expect("uninstall should succeed");

        let error = uninstall(&paths, &registry, &mut pending, "weather")
            .expect_err("a second uninstall should be rejected");

        assert!(format!("{error:#}").contains("already uninstalled"));
    }
}
