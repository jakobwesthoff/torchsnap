// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Filesystem steps of install and uninstall.
//!
//! Every function here works on `InstallPaths` alone. Deciding
//! *whether* an operation is allowed happens before these are called.

use std::path::Path;

use anyhow::Context;

use super::paths::InstallPaths;

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

/// What `remove_user_gadget` found on disk.
#[derive(Debug, PartialEq, Eq)]
pub struct Removal {
    pub archive_removed: bool,
    pub directory_removed: bool,
}

/// Delete a user gadget's archive, its hand-placed directory form if
/// any, and its state tree.
///
/// `remove_dir_all` removes a symlink itself rather than following it
/// (std behaviour since Rust 1.58.1), so a link planted inside
/// `gadget-home/<id>/` cannot redirect the deletion elsewhere.
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

    let home = paths.home(gadget_id);
    if home.exists() {
        std::fs::remove_dir_all(&home).context("remove the gadget-home directory")?;
    }

    Ok(Removal {
        archive_removed,
        directory_removed,
    })
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
        std::fs::create_dir_all(paths.home("weather")).expect("create gadget home");

        let removal = remove_user_gadget(&paths, "weather").expect("removal should succeed");

        assert_eq!(
            removal,
            Removal {
                archive_removed: false,
                directory_removed: false,
            }
        );
        assert!(!paths.home("weather").exists());
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
}
