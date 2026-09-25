// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget Install / Uninstall
//
// Backend for installing, replacing, undoing and uninstalling
// user gadgets. Every install arrives through the install queue
// (`queue.rs`), which stages the archive, shows a review and
// installs on confirmation, whatever the entry point (settings
// picker or drop zone, Finder, command line). All operations
// return `requires_restart: true`
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
// Install steps (queue request → `install_staged`):
//
// 1. Copy the source into the staging area and open the copy
//    with `ArchiveSource::open`, which validates the zip and
//    parses the manifest (`staging::StagingArea::stage`). The
//    path guard that runs during `Manifest::parse` catches
//    traversal here. Every later step uses the staged copy, so
//    the file the user picked can no longer change what lands.
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
// 2. Keep the archive that loaded at startup as the backup for
//    undo, remove any archive installed since and the directory
//    form, and leave an uninstall marker
//    (`archive_ops::remove_user_gadget`).
//    The gadget keeps running until restart, so its state tree
//    and settings are deleted by the next startup
//    (`archive_ops::process_uninstall_markers`), before any
//    gadget loads.
// 3. Record the uninstall in `PendingChanges`.
// =========================================================

mod archive_ops;
pub mod commands;
mod decision;
mod intake;
mod paths;
mod pending;
mod provenance;
mod queue;
mod registered;
mod review;
mod staging;
mod store;

pub use archive_ops::{process_uninstall_markers, remove_stale_backups, take_reopen_marker};
pub use commands::{
    QUEUE_CHANGED_EVENT, process_in_background, submit_command_line, submit_opened_urls,
};
pub use paths::InstallPaths;
pub use pending::PendingChanges;
pub use queue::{InstallQueue, QueueContext};
pub use registered::RegisteredGadgets;
pub use staging::StagingArea;
pub use store::SettingsKeys;

use std::sync::{Arc, Mutex};

use anyhow::Context;
use serde::Serialize;

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
/// install left no archive for the gadget. `requires_restart` is false
/// when the gadget is back to what loaded at startup.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub restored_version: Option<String>,
    pub requires_restart: bool,
}

// =========================================================
// Tauri commands
// =========================================================

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

/// A gadget with a change that waits for a restart, as the settings
/// panel lists it.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingGadget {
    pub kind: pending::PendingKind,
    pub name: String,
    pub description: String,
    /// The version on disk, which loads after the restart. `None` once
    /// the gadget is uninstalled.
    pub version: Option<String>,
    /// The version that runs until the restart, if one runs.
    pub previous_version: Option<String>,
}

/// Every pending change with what the settings list shows for it.
/// Names and versions of installed archives come from their manifests;
/// an uninstalled gadget is described by its startup registration.
fn pending_overview(
    pending: &PendingChanges,
    registered: &RegisteredGadgets,
) -> std::collections::HashMap<String, PendingGadget> {
    pending
        .overview()
        .into_iter()
        .filter_map(|(id, kind)| {
            let running = match registered.get(&id) {
                Some(registered::Registration::User { manifest, .. }) => Some(manifest.as_ref()),
                _ => None,
            };
            let on_disk = pending.get(&id).and_then(PendingChange::manifest);
            let described = on_disk.or(running)?;
            let previous_version = match kind {
                pending::PendingKind::Installed => None,
                _ => running.map(|manifest| manifest.gadget.version.clone()),
            };
            Some((
                id,
                PendingGadget {
                    kind,
                    name: described.gadget.name.clone(),
                    description: described.gadget.description.clone(),
                    version: on_disk.map(|manifest| manifest.gadget.version.clone()),
                    previous_version,
                },
            ))
        })
        .collect()
}

/// Changes made since startup that wait for a restart, keyed by
/// gadget id, for the settings list.
#[tauri::command]
pub fn pending_gadget_changes(
    pending: tauri::State<'_, Arc<Mutex<PendingChanges>>>,
    registered: tauri::State<'_, RegisteredGadgets>,
) -> std::collections::HashMap<String, PendingGadget> {
    let pending = pending
        .lock()
        .expect("pending changes lock is never poisoned");
    pending_overview(&pending, &registered)
}

/// Permissions of every loaded WASM gadget, keyed by gadget id,
/// for the gadget cards in the settings.
#[tauri::command]
pub fn gadget_permissions(
    registry: tauri::State<'_, crate::wasm::protocol::GadgetSourceRegistry>,
) -> std::collections::HashMap<String, Vec<review::PermissionItem>> {
    let sources = registry
        .read()
        .expect("source registry lock is never poisoned");
    review::installed_gadget_permissions(&sources)
}

// =========================================================
// Restart
//
// "Restart now" in the settings leaves a marker before restarting.
// The next setup consumes it and opens Settings on the Gadgets
// section, where the list shows the applied changes.
// =========================================================

/// The section the settings window shows next, handed out once to the
/// first settings frontend that asks. Set at startup after "Restart
/// now", and by `crate::show_settings_window_at` at runtime.
pub struct SettingsStartSection(Mutex<Option<String>>);

impl SettingsStartSection {
    pub fn new(section: Option<&str>) -> Self {
        Self(Mutex::new(section.map(str::to_string)))
    }

    /// Replace whatever section is waiting, so the latest request wins.
    pub fn set(&self, section: &str) {
        *self.0.lock().expect("start section lock is never poisoned") = Some(section.to_string());
    }

    pub fn take(&self) -> Option<String> {
        self.0
            .lock()
            .expect("start section lock is never poisoned")
            .take()
    }
}

#[tauri::command]
pub fn take_settings_start_section(
    start: tauri::State<'_, SettingsStartSection>,
) -> Option<String> {
    start.take()
}

/// Restart the app to load pending gadget changes. A marker that
/// cannot be written only costs the reopened Settings window, so the
/// restart goes ahead.
#[tauri::command]
pub fn restart_to_apply_gadget_changes(
    app: tauri::AppHandle,
    paths: tauri::State<'_, InstallPaths>,
) {
    if let Err(e) = archive_ops::write_reopen_marker(&paths) {
        eprintln!("failed to leave the reopen marker for Settings: {e:#}");
    }
    app.restart()
}

// =========================================================
// Install
// =========================================================

fn install_staged(
    paths: &InstallPaths,
    registered: &RegisteredGadgets,
    pending: &mut PendingChanges,
    staged: &staging::StagedArchive,
) -> anyhow::Result<InstalledGadgetInfo> {
    let manifest = staged.manifest().clone();
    let gadget_id = manifest.gadget.id.as_str().to_string();

    let decision = decide_install(
        registered.get(&gadget_id),
        pending.get(&gadget_id),
        &manifest,
    );

    let (previous_version, version_relation) = match decision {
        InstallDecision::Reject(message) => anyhow::bail!(message),
        InstallDecision::Fresh => {
            archive_ops::publish_fresh(paths, staged.path(), &gadget_id)?;
            pending.record_install(&gadget_id, manifest.clone());
            (None, None)
        }
        InstallDecision::Replace { previous, relation } => {
            archive_ops::publish_replace(paths, staged.path(), &gadget_id)?;
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

/// Reverse the install, replace or uninstall of `gadget_id` made in
/// this session. The backup written by the first replace decides what
/// comes back; without one, the install was fresh and its archive
/// is removed.
fn undo(
    paths: &InstallPaths,
    registered: &RegisteredGadgets,
    pending: &mut PendingChanges,
    gadget_id: &str,
) -> anyhow::Result<UndoResult> {
    let Some(change) = pending.get(gadget_id).cloned() else {
        anyhow::bail!("nothing to undo for `{gadget_id}` in this session");
    };

    let restored_version = match change {
        // The backup is the archive that loaded at startup, so the
        // startup state is back and nothing is pending any more.
        PendingChange::Replaced { .. } => {
            archive_ops::restore_backup(paths, gadget_id)?;
            pending.forget(gadget_id);
            Some(manifest_on_disk(paths, gadget_id)?.gadget.version)
        }
        // A backup here is the first install of this session, replaced
        // since; without one the install was the only one and goes.
        PendingChange::Installed { .. } => {
            if archive_ops::restore_backup(paths, gadget_id)? {
                let manifest = manifest_on_disk(paths, gadget_id)?;
                let version = manifest.gadget.version.clone();
                pending.record_install(gadget_id, manifest);
                Some(version)
            } else {
                archive_ops::remove_archive(paths, gadget_id)?;
                pending.forget(gadget_id);
                None
            }
        }
        // Back to "uninstalled": the startup archive stays in its backup
        // and the uninstall marker stays in place.
        PendingChange::Reinstalled { .. } => {
            archive_ops::remove_archive(paths, gadget_id)?;
            pending.record_uninstall(gadget_id, registered.get(gadget_id).is_some());
            None
        }
        // Uninstall kept the startup archive as a backup; putting it back
        // and removing the marker leaves nothing for the next start.
        PendingChange::Uninstalled => {
            if !archive_ops::restore_backup(paths, gadget_id)? {
                anyhow::bail!(
                    "the uninstall of `{gadget_id}` cannot be undone: it was installed as a directory, which is not kept"
                );
            }
            archive_ops::remove_marker(paths, gadget_id)?;
            pending.forget(gadget_id);
            Some(manifest_on_disk(paths, gadget_id)?.gadget.version)
        }
    };

    // Anything still recorded for the gadget (an earlier install, or
    // an uninstall that a reinstall had covered) only takes effect on
    // restart; without a record the startup state is back.
    Ok(UndoResult {
        restored_version,
        requires_restart: pending.get(gadget_id).is_some(),
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

    let removal = archive_ops::remove_user_gadget(paths, gadget_id, registration.is_some())?;
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
    use std::path::Path;

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

    /// Stage `archive_path` next to the test's gadgets dir and install
    /// the staged copy, the way a confirmed queue request does.
    fn install(
        paths: &InstallPaths,
        registered: &RegisteredGadgets,
        pending: &mut PendingChanges,
        archive_path: &Path,
    ) -> anyhow::Result<InstalledGadgetInfo> {
        let staging = StagingArea::new(paths.gadgets_dir.with_file_name("install-staging"));
        install_via(paths, registered, pending, &staging, archive_path)
    }

    fn install_via(
        paths: &InstallPaths,
        registered: &RegisteredGadgets,
        pending: &mut PendingChanges,
        staging: &StagingArea,
        archive_path: &Path,
    ) -> anyhow::Result<InstalledGadgetInfo> {
        let staged = staging.stage(archive_path)?;
        let result = install_staged(paths, registered, pending, &staged);
        staged.discard();
        result
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
                "A bundled gadget with id `weather` ships with Torchsnap",
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
        // The version that loaded at startup is back on disk.
        assert!(!undone.requires_restart);
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
        assert!(!undone.requires_restart);
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
        // The first install is still new since startup.
        assert!(undone.requires_restart);
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

        let undone = undo(&paths, &registry, &mut pending, "weather").expect("undo should succeed");

        // The uninstall from before the reinstall still waits for a restart.
        assert!(undone.requires_restart);
        assert!(!paths.archive("weather").exists());
        assert!(paths.uninstall_marker("weather").exists());
        assert!(matches!(
            pending.get("weather"),
            Some(pending::PendingChange::Uninstalled)
        ));
    }

    #[test]
    fn undo_of_an_uninstall_brings_the_gadget_back() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        let mut pending = PendingChanges::default();
        uninstall(&paths, &registry, &mut pending, "weather").expect("uninstall");

        let undone = undo(&paths, &registry, &mut pending, "weather").expect("undo should succeed");

        assert_eq!(undone.restored_version.as_deref(), Some("1.0.0"));
        assert!(!undone.requires_restart);
        assert_eq!(
            manifest_of(&paths.archive("weather")).gadget.version,
            "1.0.0"
        );
        assert!(!paths.uninstall_marker("weather").exists());
        assert!(pending.get("weather").is_none());
    }

    #[test]
    fn undo_of_an_uninstall_after_a_replace_returns_to_the_startup_version() {
        let (_root, paths) = temp_paths();
        let registry = registered_user_gadget(&paths, "weather", "1.0.0");
        let mut pending = PendingChanges::default();
        let (_src, v2) = archive_with_manifest("weather", "2.0.0", "");
        install(&paths, &registry, &mut pending, &v2).expect("replace");
        uninstall(&paths, &registry, &mut pending, "weather").expect("uninstall");

        let undone = undo(&paths, &registry, &mut pending, "weather").expect("undo should succeed");

        assert_eq!(undone.restored_version.as_deref(), Some("1.0.0"));
        assert!(pending.get("weather").is_none());
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
        assert_eq!(
            dir_entries(&paths.gadgets_dir),
            vec![".weather.torchsnap.prev", ".weather.uninstall"]
        );
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

    // =========================================================
    // Staging
    // =========================================================

    #[test]
    fn install_leaves_no_staged_copy_behind() {
        let (root, paths) = temp_paths();
        let staging = StagingArea::new(root.path().join("install-staging"));
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");

        install_via(
            &paths,
            &RegisteredGadgets::default(),
            &mut PendingChanges::default(),
            &staging,
            &archive,
        )
        .expect("install should succeed");

        assert!(dir_entries(&root.path().join("install-staging")).is_empty());
    }

    #[test]
    fn a_rejected_install_leaves_no_staged_copy_behind() {
        let (root, paths) = temp_paths();
        let staging = StagingArea::new(root.path().join("install-staging"));
        let (_src, archive) = archive_with_manifest("clipboard-manager", "1.0.0", "");
        let registry = RegisteredGadgets::from_entries([(
            "clipboard-manager".to_string(),
            Registration::Builtin,
        )]);

        install_via(
            &paths,
            &registry,
            &mut PendingChanges::default(),
            &staging,
            &archive,
        )
        .expect_err("builtin id must be rejected");

        assert!(dir_entries(&root.path().join("install-staging")).is_empty());
    }

    // =========================================================
    // Pending overview
    // =========================================================

    #[test]
    fn the_pending_overview_names_each_gadget_and_its_versions() {
        let (_root, paths) = temp_paths();
        let mut registry = registered_user_gadget(&paths, "weather", "1.0.0");
        registry = RegisteredGadgets::from_entries([
            (
                "weather".to_string(),
                registry.get("weather").cloned().expect("registered"),
            ),
            (
                "zerotier".to_string(),
                registered_user_gadget(&paths, "zerotier", "0.1.0")
                    .get("zerotier")
                    .cloned()
                    .expect("registered"),
            ),
        ]);
        let mut pending = PendingChanges::default();
        let (_a, weather_v2) = archive_with_manifest("weather", "2.0.0", "");
        let (_b, calendar) = archive_with_manifest("calendar", "1.0.0", "");
        install(&paths, &registry, &mut pending, &weather_v2).expect("replace");
        install(&paths, &registry, &mut pending, &calendar).expect("fresh install");
        uninstall(&paths, &registry, &mut pending, "zerotier").expect("uninstall");

        let overview = pending_overview(&pending, &registry);

        let weather = &overview["weather"];
        assert_eq!(weather.kind, pending::PendingKind::Replaced);
        assert_eq!(weather.version.as_deref(), Some("2.0.0"));
        assert_eq!(weather.previous_version.as_deref(), Some("1.0.0"));

        let calendar = &overview["calendar"];
        assert_eq!(calendar.kind, pending::PendingKind::Installed);
        assert_eq!(calendar.name, "Gadget calendar");
        assert_eq!(calendar.version.as_deref(), Some("1.0.0"));
        assert_eq!(calendar.previous_version, None);

        let zerotier = &overview["zerotier"];
        assert_eq!(zerotier.kind, pending::PendingKind::Uninstalled);
        assert_eq!(zerotier.version, None);
        assert_eq!(zerotier.previous_version.as_deref(), Some("0.1.0"));

        assert_eq!(
            serde_json::to_value(zerotier).expect("serializes"),
            serde_json::json!({
                "kind": "uninstalled",
                "name": "Gadget zerotier",
                "description": "Test gadget zerotier",
                "version": null,
                "previousVersion": "0.1.0"
            })
        );
    }

    #[test]
    fn the_settings_start_section_is_handed_out_once() {
        let start = SettingsStartSection::new(Some("gadgets"));

        assert_eq!(start.take().as_deref(), Some("gadgets"));
        assert_eq!(start.take(), None);
        assert_eq!(SettingsStartSection::new(None).take(), None);
    }

    #[test]
    fn a_requested_settings_section_replaces_the_waiting_one() {
        let start = SettingsStartSection::new(Some("gadgets"));

        start.set("zerotier");

        assert_eq!(start.take().as_deref(), Some("zerotier"));
        assert_eq!(start.take(), None);
    }
}
