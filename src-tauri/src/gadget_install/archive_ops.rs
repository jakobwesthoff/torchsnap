// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Filesystem steps of install and uninstall, and the startup
//! cleanup that finishes an uninstall.
//!
//! Every function here works on `InstallPaths` alone. Deciding
//! *whether* an operation is allowed happens before these are called.

use std::path::Path;

use anyhow::Context;

use super::paths::{BACKUP_SUFFIX, InstallPaths, UNINSTALL_MARKER_SUFFIX};
use super::store::{SettingsKeys, strip_gadget_settings};
use crate::wasm::manifest::validate_gadget_id;

/// Place `source` at `gadgets/<id>.torchsnap`.
///
/// The copy goes to a dot-prefixed temp file in the same directory
/// first and is then renamed into place. Rename within one filesystem
/// is atomic, so a crash mid-copy leaves only the temp file, which
/// startup discovery skips because it is not a plain `*.torchsnap`
/// entry.
pub fn publish_fresh(paths: &InstallPaths, source: &Path, gadget_id: &str) -> anyhow::Result<()> {
    std::fs::create_dir_all(&paths.gadgets_dir).context("create the user gadgets directory")?;

    let tmp_path = paths
        .gadgets_dir
        .join(format!(".{gadget_id}.torchsnap.tmp"));
    std::fs::copy(source, &tmp_path).context("copy the archive into its staging location")?;
    std::fs::rename(&tmp_path, paths.archive(gadget_id))
        .context("move the staged archive into place")?;
    Ok(())
}

/// Swap the archive at `gadgets/<id>.torchsnap` for `source`, keeping
/// the current one as `.<id>.torchsnap.prev` for undo.
///
/// The backup is written only if none exists yet, so it always holds
/// what was on disk before the first replace of this session: the
/// version that runs until restart, or the first pending install. The
/// current archive is renamed rather than copied, which keeps its
/// inode, so a running gadget holding it open keeps reading the same
/// bytes.
pub fn publish_replace(paths: &InstallPaths, source: &Path, gadget_id: &str) -> anyhow::Result<()> {
    let archive = paths.archive(gadget_id);
    let backup = paths.backup(gadget_id);
    if !backup.exists() && archive.exists() {
        std::fs::rename(&archive, &backup).context("keep the replaced archive as a backup")?;
    }
    publish_fresh(paths, source, gadget_id)
}

/// What `undo_publish` did.
#[derive(Debug, PartialEq, Eq)]
pub enum Undone {
    /// The backup of a replaced archive is back in place.
    Restored,
    /// There was no backup, so the fresh install was removed.
    Removed,
}

/// Reverse the last publish for `gadget_id` in this session.
pub fn undo_publish(paths: &InstallPaths, gadget_id: &str) -> anyhow::Result<Undone> {
    let archive = paths.archive(gadget_id);
    let backup = paths.backup(gadget_id);
    if backup.exists() {
        std::fs::rename(&backup, &archive).context("restore the replaced archive")?;
        Ok(Undone::Restored)
    } else {
        if archive.exists() {
            std::fs::remove_file(&archive).context("remove the installed archive")?;
        }
        Ok(Undone::Removed)
    }
}

/// What `remove_user_gadget` found on disk.
#[derive(Debug, PartialEq, Eq)]
pub struct Removal {
    pub archive_removed: bool,
    pub directory_removed: bool,
}

/// Delete a user gadget's archive and its hand-placed directory form
/// if any, and leave an uninstall marker for the next startup.
///
/// The gadget keeps running until restart, with its SQLite connection
/// open and its settings watched. Deleting `gadget-home/<id>/` or its
/// settings now would pull data out from under a live instance, which
/// can write it back. The marker defers that cleanup to
/// `process_uninstall_markers`, which runs before any gadget loads.
pub fn remove_user_gadget(paths: &InstallPaths, gadget_id: &str) -> anyhow::Result<Removal> {
    let archive = paths.archive(gadget_id);
    let archive_removed = archive.exists();
    if archive_removed {
        std::fs::remove_file(&archive).context("remove the gadget archive")?;
    }

    let directory = paths.directory(gadget_id);
    let directory_removed = directory.exists();
    if directory_removed {
        std::fs::remove_dir_all(&directory).context("remove the gadget directory")?;
    }

    // Without this, undoing a later fresh install of the same id would
    // bring back an archive from before the uninstall.
    let backup = paths.backup(gadget_id);
    if backup.exists() {
        std::fs::remove_file(&backup).context("remove the replaced archive's backup")?;
    }

    std::fs::create_dir_all(&paths.gadgets_dir).context("create the user gadgets directory")?;
    std::fs::write(paths.uninstall_marker(gadget_id), b"").context("write the uninstall marker")?;

    Ok(Removal {
        archive_removed,
        directory_removed,
    })
}

/// Delete the replace backups of the previous session. Undo only
/// works until restart, and after a restart the archive on disk is
/// the one that runs.
pub fn remove_stale_backups(paths: &InstallPaths) -> anyhow::Result<()> {
    let entries = match std::fs::read_dir(&paths.gadgets_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).context("read the user gadgets directory"),
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        let is_backup = entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with('.') && name.ends_with(BACKUP_SUFFIX));
        if is_backup {
            std::fs::remove_file(entry.path()).context("remove a stale replace backup")?;
        }
    }
    Ok(())
}

/// Finish every uninstall from the previous session: delete the
/// gadget's state tree and settings, then its marker. Returns the ids
/// that were cleaned up.
///
/// Runs in `setup` before settings are initialized and before gadgets
/// load, so nothing is running for these ids yet. A marker is removed
/// only after its cleanup succeeded, so a failure is retried on the
/// next start. `remove_dir_all` removes a symlink itself rather than
/// following it (std behaviour since Rust 1.58.1), so a link planted
/// inside `gadget-home/<id>/` cannot redirect the deletion elsewhere.
pub fn process_uninstall_markers(
    paths: &InstallPaths,
    settings: &dyn SettingsKeys,
) -> anyhow::Result<Vec<String>> {
    let entries = match std::fs::read_dir(&paths.gadgets_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e).context("read the user gadgets directory"),
    };

    // Marker names turn into paths under `gadget-home/`, so only names
    // that are valid gadget ids (lowercase, digits, hyphens) are acted
    // on. Anything else, `..` included, is ignored.
    let mut gadget_ids: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = name
                .strip_prefix('.')?
                .strip_suffix(UNINSTALL_MARKER_SUFFIX)?;
            validate_gadget_id(id).ok()?;
            Some(id.to_string())
        })
        .collect();
    gadget_ids.sort();

    for gadget_id in &gadget_ids {
        let home = paths.home(gadget_id);
        if home.exists() {
            std::fs::remove_dir_all(&home)
                .with_context(|| format!("remove the gadget-home directory of `{gadget_id}`"))?;
        }
        strip_gadget_settings(settings, gadget_id)?;
        std::fs::remove_file(paths.uninstall_marker(gadget_id))
            .with_context(|| format!("remove the uninstall marker of `{gadget_id}`"))?;
    }

    Ok(gadget_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, InstallPaths) {
        let root = tempfile::tempdir().expect("create temp app data dir");
        let paths = InstallPaths::new(root.path());
        (root, paths)
    }

    #[test]
    fn publish_fresh_leaves_only_the_final_archive() {
        let (_root, paths) = temp_paths();
        let source = tempfile::NamedTempFile::new().expect("create source file");
        std::fs::write(source.path(), b"archive bytes").expect("write source");

        publish_fresh(&paths, source.path(), "weather").expect("publish should succeed");

        let entries: Vec<_> = std::fs::read_dir(&paths.gadgets_dir)
            .expect("gadgets dir exists")
            .map(|e| e.expect("dir entry").file_name())
            .collect();
        assert_eq!(entries, vec!["weather.torchsnap"]);
        assert_eq!(
            std::fs::read(paths.archive("weather")).expect("read published"),
            b"archive bytes"
        );
    }

    #[test]
    fn remove_reports_an_absent_archive() {
        let (_root, paths) = temp_paths();

        let removal = remove_user_gadget(&paths, "weather").expect("removal should succeed");

        assert_eq!(
            removal,
            Removal {
                archive_removed: false,
                directory_removed: false,
            }
        );
    }

    #[test]
    fn remove_reports_both_forms_when_present() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(paths.directory("weather")).expect("create directory form");
        std::fs::write(paths.archive("weather"), b"archive").expect("write archive");

        let removal = remove_user_gadget(&paths, "weather").expect("removal should succeed");

        assert_eq!(
            removal,
            Removal {
                archive_removed: true,
                directory_removed: true,
            }
        );
    }

    // =========================================================
    // Deferred uninstall cleanup
    // =========================================================

    use crate::gadget_install::store::{MemorySettings, SettingsKeys};

    fn with_gadget_state(paths: &InstallPaths, id: &str) {
        std::fs::create_dir_all(paths.home(id)).expect("create gadget home");
        std::fs::write(paths.home(id).join("storage.sqlite3"), b"db").expect("write gadget data");
    }

    #[test]
    fn remove_keeps_the_state_and_leaves_a_marker() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"archive").expect("write archive");
        with_gadget_state(&paths, "weather");

        remove_user_gadget(&paths, "weather").expect("removal should succeed");

        assert!(!paths.archive("weather").exists());
        assert!(paths.home("weather").join("storage.sqlite3").exists());
        assert!(paths.uninstall_marker("weather").exists());
    }

    #[test]
    fn startup_processing_deletes_state_settings_and_marker() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        with_gadget_state(&paths, "weather");
        std::fs::write(paths.uninstall_marker("weather"), b"").expect("write marker");
        let settings = MemorySettings::with_keys(&[
            "enabled.weather",
            "gadgets.weather.city",
            "appearance.theme",
        ]);

        let cleaned =
            process_uninstall_markers(&paths, &settings).expect("processing should succeed");

        assert_eq!(cleaned, vec!["weather"]);
        assert!(!paths.home("weather").exists());
        assert!(!paths.uninstall_marker("weather").exists());
        assert_eq!(settings.keys(), vec!["appearance.theme"]);
    }

    /// Uninstall followed by reinstall before restart: the new archive
    /// stays, and it starts with empty data.
    #[test]
    fn startup_processing_keeps_a_reinstalled_archive() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        with_gadget_state(&paths, "weather");
        std::fs::write(paths.uninstall_marker("weather"), b"").expect("write marker");
        std::fs::write(paths.archive("weather"), b"new archive")
            .expect("write reinstalled archive");
        let settings = MemorySettings::with_keys(&[]);

        process_uninstall_markers(&paths, &settings).expect("processing should succeed");

        assert!(paths.archive("weather").exists());
        assert!(!paths.home("weather").exists());
    }

    #[test]
    fn startup_processing_twice_is_harmless() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        with_gadget_state(&paths, "weather");
        std::fs::write(paths.uninstall_marker("weather"), b"").expect("write marker");
        let settings = MemorySettings::with_keys(&[]);

        process_uninstall_markers(&paths, &settings).expect("first run should succeed");
        let second =
            process_uninstall_markers(&paths, &settings).expect("second run should succeed");

        assert!(second.is_empty());
    }

    #[test]
    fn startup_processing_leaves_other_gadgets_alone() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        with_gadget_state(&paths, "weather");
        with_gadget_state(&paths, "calendar");
        std::fs::write(paths.uninstall_marker("weather"), b"").expect("write marker");
        let settings = MemorySettings::with_keys(&["enabled.calendar"]);

        process_uninstall_markers(&paths, &settings).expect("processing should succeed");

        assert!(paths.home("calendar").join("storage.sqlite3").exists());
        assert_eq!(settings.keys(), vec!["enabled.calendar"]);
    }

    /// Marker names become paths under `gadget-home/`, so a name that
    /// is not a valid gadget id (`..`, uppercase, spaces) must never
    /// be acted on.
    #[test]
    fn startup_processing_ignores_markers_with_invalid_ids() {
        let (root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::create_dir_all(root.path().join("precious")).expect("create unrelated dir");
        for name in [
            "...uninstall",
            ".Weather.uninstall",
            ".we ather.uninstall",
            ".uninstall",
        ] {
            std::fs::write(paths.gadgets_dir.join(name), b"").expect("write bogus marker");
        }
        let settings = MemorySettings::with_keys(&[]);

        let cleaned =
            process_uninstall_markers(&paths, &settings).expect("processing should succeed");

        assert!(cleaned.is_empty());
        assert!(root.path().join("precious").exists());
    }

    /// If the settings cannot be saved the marker stays, so the next
    /// start retries the cleanup instead of forgetting it.
    #[test]
    fn startup_processing_keeps_the_marker_when_saving_fails() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.uninstall_marker("weather"), b"").expect("write marker");
        let settings = MemorySettings::with_keys(&["enabled.weather"]).failing_save();

        assert!(process_uninstall_markers(&paths, &settings).is_err());
        assert!(paths.uninstall_marker("weather").exists());
    }

    #[test]
    fn startup_processing_without_a_gadgets_dir_does_nothing() {
        let (_root, paths) = temp_paths();
        let settings = MemorySettings::with_keys(&[]);

        let cleaned =
            process_uninstall_markers(&paths, &settings).expect("processing should succeed");

        assert!(cleaned.is_empty());
    }

    // =========================================================
    // Replace, backup and undo
    // =========================================================

    fn source_file(contents: &[u8]) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().expect("create source file");
        std::fs::write(file.path(), contents).expect("write source");
        file
    }

    fn read(path: &std::path::Path) -> Vec<u8> {
        std::fs::read(path).expect("read file")
    }

    #[test]
    fn replace_backs_up_the_current_archive_and_publishes_the_new_one() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v1").expect("write installed archive");
        let v2 = source_file(b"v2");

        publish_replace(&paths, v2.path(), "weather").expect("replace should succeed");

        assert_eq!(read(&paths.archive("weather")), b"v2");
        assert_eq!(read(&paths.backup("weather")), b"v1");
    }

    /// The backup holds what ran when the session started, so a second
    /// replace must not overwrite it with the intermediate version.
    #[test]
    fn a_second_replace_keeps_the_original_backup() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v1").expect("write installed archive");
        publish_replace(&paths, source_file(b"v2").path(), "weather").expect("first replace");

        publish_replace(&paths, source_file(b"v3").path(), "weather").expect("second replace");

        assert_eq!(read(&paths.archive("weather")), b"v3");
        assert_eq!(read(&paths.backup("weather")), b"v1");
    }

    /// Renaming keeps the inode, so a running gadget that holds the old
    /// archive open keeps reading the same bytes after the replace.
    #[cfg(unix)]
    #[test]
    fn replace_keeps_the_old_archive_readable_through_an_open_handle() {
        use std::io::Read as _;

        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v1").expect("write installed archive");
        let mut open_handle = std::fs::File::open(paths.archive("weather")).expect("open archive");

        publish_replace(&paths, source_file(b"v2").path(), "weather").expect("replace");

        let mut contents = Vec::new();
        open_handle
            .read_to_end(&mut contents)
            .expect("read through old handle");
        assert_eq!(contents, b"v1");
    }

    #[test]
    fn undo_restores_the_backup() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v1").expect("write installed archive");
        publish_replace(&paths, source_file(b"v2").path(), "weather").expect("replace");

        let undone = undo_publish(&paths, "weather").expect("undo should succeed");

        assert_eq!(undone, Undone::Restored);
        assert_eq!(read(&paths.archive("weather")), b"v1");
        assert!(!paths.backup("weather").exists());
    }

    #[test]
    fn undo_without_a_backup_removes_the_fresh_install() {
        let (_root, paths) = temp_paths();
        publish_fresh(&paths, source_file(b"v1").path(), "weather").expect("fresh install");

        let undone = undo_publish(&paths, "weather").expect("undo should succeed");

        assert_eq!(undone, Undone::Removed);
        assert!(!paths.archive("weather").exists());
    }

    #[test]
    fn uninstall_removes_the_backup() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v1").expect("write installed archive");
        publish_replace(&paths, source_file(b"v2").path(), "weather").expect("replace");

        remove_user_gadget(&paths, "weather").expect("removal should succeed");

        assert!(!paths.backup("weather").exists());
        assert!(!paths.archive("weather").exists());
    }

    #[test]
    fn startup_removes_leftover_backups_only() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.backup("weather"), b"v1").expect("write backup");
        std::fs::write(paths.archive("weather"), b"v2").expect("write archive");
        std::fs::write(paths.uninstall_marker("calendar"), b"").expect("write marker");

        remove_stale_backups(&paths).expect("sweep should succeed");

        assert!(!paths.backup("weather").exists());
        assert!(paths.archive("weather").exists());
        assert!(paths.uninstall_marker("calendar").exists());
    }

    #[test]
    fn startup_backup_sweep_without_a_gadgets_dir_does_nothing() {
        let (_root, paths) = temp_paths();

        remove_stale_backups(&paths).expect("sweep should succeed");
    }
}
