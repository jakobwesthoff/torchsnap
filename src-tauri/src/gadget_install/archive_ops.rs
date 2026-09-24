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

/// Put `.<id>.torchsnap.prev` back as `<id>.torchsnap`. Returns
/// false when there is no backup to restore.
pub fn restore_backup(paths: &InstallPaths, gadget_id: &str) -> anyhow::Result<bool> {
    let backup = paths.backup(gadget_id);
    if !backup.exists() {
        return Ok(false);
    }
    std::fs::rename(&backup, paths.archive(gadget_id)).context("restore the backed-up archive")?;
    Ok(true)
}

/// Delete `<id>.torchsnap` if it exists.
pub fn remove_archive(paths: &InstallPaths, gadget_id: &str) -> anyhow::Result<()> {
    let archive = paths.archive(gadget_id);
    if archive.exists() {
        std::fs::remove_file(&archive).context("remove the installed archive")?;
    }
    Ok(())
}

/// Delete the uninstall marker if it exists, cancelling the cleanup
/// the next startup would do.
pub fn remove_marker(paths: &InstallPaths, gadget_id: &str) -> anyhow::Result<()> {
    let marker = paths.uninstall_marker(gadget_id);
    if marker.exists() {
        std::fs::remove_file(&marker).context("remove the uninstall marker")?;
    }
    Ok(())
}

/// What `remove_user_gadget` found on disk.
#[derive(Debug, PartialEq, Eq)]
pub struct Removal {
    pub archive_removed: bool,
    pub directory_removed: bool,
}

/// Take a user gadget off disk.
///
/// For a gadget that loaded at startup (`registered`), the archive that
/// loaded is kept as `.<id>.torchsnap.prev` so the uninstall can be
/// undone until restart: if a replace already put it there, the newer
/// archive is simply deleted. An uninstall marker then tells the next
/// startup to delete the gadget's data and settings; the gadget keeps
/// running until restart, and deleting them now would pull data out
/// from under the live instance, which can write it back. A gadget that
/// only existed as an install of this session has no data yet, so its
/// archive and any backup are deleted outright and no marker is needed.
///
/// A hand-placed directory form is deleted in either case and cannot
/// be restored by undo.
pub fn remove_user_gadget(
    paths: &InstallPaths,
    gadget_id: &str,
    registered: bool,
) -> anyhow::Result<Removal> {
    let archive = paths.archive(gadget_id);
    let backup = paths.backup(gadget_id);
    let archive_removed = archive.exists();
    if archive_removed {
        if registered && !backup.exists() {
            std::fs::rename(&archive, &backup).context("keep the gadget archive for undo")?;
        } else {
            std::fs::remove_file(&archive).context("remove the gadget archive")?;
        }
    }
    if !registered && backup.exists() {
        std::fs::remove_file(&backup).context("remove the replaced archive's backup")?;
    }

    let directory = paths.directory(gadget_id);
    let directory_removed = directory.exists();
    if directory_removed {
        std::fs::remove_dir_all(&directory).context("remove the gadget directory")?;
    }

    if registered {
        std::fs::create_dir_all(&paths.gadgets_dir).context("create the user gadgets directory")?;
        std::fs::write(paths.uninstall_marker(gadget_id), b"")
            .context("write the uninstall marker")?;
    }

    Ok(Removal {
        archive_removed,
        directory_removed,
    })
}

/// Remember that the next start should open Settings on the Gadgets
/// section, so a user who restarted to apply gadget changes sees them.
pub fn write_reopen_marker(paths: &InstallPaths) -> anyhow::Result<()> {
    if let Some(parent) = paths.reopen_settings_marker.parent() {
        std::fs::create_dir_all(parent).context("create the app data directory")?;
    }
    std::fs::write(&paths.reopen_settings_marker, b"").context("write the reopen marker")
}

/// Consume the reopen marker. True when it was there.
pub fn take_reopen_marker(paths: &InstallPaths) -> anyhow::Result<bool> {
    match std::fs::remove_file(&paths.reopen_settings_marker) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("remove the reopen marker"),
    }
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

        let removal = remove_user_gadget(&paths, "weather", false).expect("removal should succeed");

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

        let removal = remove_user_gadget(&paths, "weather", false).expect("removal should succeed");

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
    fn uninstalling_a_registered_gadget_keeps_its_archive_for_undo() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"archive").expect("write archive");
        with_gadget_state(&paths, "weather");

        remove_user_gadget(&paths, "weather", true).expect("removal should succeed");

        assert!(!paths.archive("weather").exists());
        assert_eq!(read(&paths.backup("weather")), b"archive");
        assert!(paths.home("weather").join("storage.sqlite3").exists());
        assert!(paths.uninstall_marker("weather").exists());
    }

    /// After a replace the backup already holds the archive that loaded
    /// at startup; uninstalling keeps that one and drops the newer copy.
    #[test]
    fn uninstalling_after_a_replace_keeps_the_startup_backup() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v1").expect("write archive");
        publish_replace(&paths, source_file(b"v2").path(), "weather").expect("replace");

        remove_user_gadget(&paths, "weather", true).expect("removal should succeed");

        assert!(!paths.archive("weather").exists());
        assert_eq!(read(&paths.backup("weather")), b"v1");
    }

    #[test]
    fn uninstalling_a_gadget_that_never_loaded_leaves_nothing_behind() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v2").expect("write archive");
        std::fs::write(paths.backup("weather"), b"v1").expect("write backup");

        remove_user_gadget(&paths, "weather", false).expect("removal should succeed");

        assert!(!paths.archive("weather").exists());
        assert!(!paths.backup("weather").exists());
        assert!(!paths.uninstall_marker("weather").exists());
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
    fn restoring_the_backup_puts_it_back_in_place() {
        let (_root, paths) = temp_paths();
        std::fs::create_dir_all(&paths.gadgets_dir).expect("create gadgets dir");
        std::fs::write(paths.archive("weather"), b"v1").expect("write installed archive");
        publish_replace(&paths, source_file(b"v2").path(), "weather").expect("replace");

        let restored = restore_backup(&paths, "weather").expect("restore should succeed");

        assert!(restored);
        assert_eq!(read(&paths.archive("weather")), b"v1");
        assert!(!paths.backup("weather").exists());
    }

    #[test]
    fn restoring_without_a_backup_reports_it() {
        let (_root, paths) = temp_paths();

        assert!(!restore_backup(&paths, "weather").expect("restore should succeed"));
    }

    #[test]
    fn removing_the_archive_and_the_marker() {
        let (_root, paths) = temp_paths();
        publish_fresh(&paths, source_file(b"v1").path(), "weather").expect("fresh install");
        std::fs::write(paths.uninstall_marker("weather"), b"").expect("write marker");

        remove_archive(&paths, "weather").expect("remove archive");
        remove_marker(&paths, "weather").expect("remove marker");

        assert!(!paths.archive("weather").exists());
        assert!(!paths.uninstall_marker("weather").exists());
        // Both are fine to call when there is nothing to remove.
        remove_archive(&paths, "weather").expect("remove absent archive");
        remove_marker(&paths, "weather").expect("remove absent marker");
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

    // =========================================================
    // Reopening Settings after a restart
    // =========================================================

    #[test]
    fn the_reopen_marker_is_taken_once() {
        let (_root, paths) = temp_paths();

        assert!(!take_reopen_marker(&paths).expect("take without marker"));

        write_reopen_marker(&paths).expect("write marker");

        assert!(take_reopen_marker(&paths).expect("take the marker"));
        assert!(!take_reopen_marker(&paths).expect("take again"));
    }
}
