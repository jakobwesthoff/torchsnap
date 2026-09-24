// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Private copies of archives on their way to being installed.
//!
//! The file a user picks, drops or double-clicks stays under their
//! control: it can change between the moment Torchsnap reads its
//! manifest and the moment it is copied into the gadgets directory.
//! Staging copies it once into an app-owned directory and everything
//! after that (the review, the install) works on that copy, so what
//! the user approved is exactly what gets installed.

use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::Context;

use crate::wasm::manifest::Manifest;
use crate::wasm::source::{ArchiveSource, GadgetSource};

/// Largest archive file accepted for installation. Bundled gadgets
/// are well under 1 MiB, so this leaves wide headroom while bounding
/// how much one request can write into the cache directory. It limits
/// the archive file only; decompressed sizes are a separate concern.
pub const MAX_ARCHIVE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct StagingArea {
    dir: PathBuf,
    max_bytes: u64,
}

/// An archive copied into the staging directory, with its parsed
/// manifest. Dropping it leaves the file in place (a queued request
/// may still need it); `discard` removes it.
#[derive(Debug)]
pub struct StagedArchive {
    path: PathBuf,
    manifest: Manifest,
}

impl StagingArea {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            max_bytes: MAX_ARCHIVE_BYTES,
        }
    }

    #[cfg(test)]
    pub fn with_limit(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    #[cfg(test)]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Copy `source` into the staging directory and parse the copy.
    /// On any failure the partial copy is removed.
    pub fn stage(&self, source: &Path) -> anyhow::Result<StagedArchive> {
        std::fs::create_dir_all(&self.dir).context("create the install staging directory")?;
        let staged_path = self.dir.join(format!(
            "{}.torchsnap",
            ulid::Ulid::generate().to_string().to_lowercase()
        ));

        let result = self
            .copy_capped(source, &staged_path)
            .and_then(|()| {
                ArchiveSource::open(&staged_path).context("open the staged gadget archive")
            })
            .map(|archive| archive.manifest().clone());

        match result {
            Ok(manifest) => Ok(StagedArchive {
                path: staged_path,
                manifest,
            }),
            Err(e) => {
                let _ = std::fs::remove_file(&staged_path);
                Err(e)
            }
        }
    }

    /// Copy at most `max_bytes + 1` bytes; reading one byte past the
    /// limit is how an oversized file is detected without trusting its
    /// metadata. `create_new` refuses an existing path, so a file or
    /// symlink already sitting at the staging name is never written
    /// through.
    fn copy_capped(&self, source: &Path, staged_path: &Path) -> anyhow::Result<()> {
        let input = std::fs::File::open(source).context("open the gadget archive")?;
        let mut output = std::fs::File::options()
            .write(true)
            .create_new(true)
            .open(staged_path)
            .context("create the staged copy")?;
        let copied = std::io::copy(&mut input.take(self.max_bytes + 1), &mut output)
            .context("copy the gadget archive into staging")?;
        if copied > self.max_bytes {
            anyhow::bail!(
                "the gadget archive is larger than the {} MiB limit",
                self.max_bytes / (1024 * 1024)
            );
        }
        Ok(())
    }

    /// Remove everything left in the staging directory. Runs at
    /// startup, when no request can still refer to a staged file.
    pub fn sweep(&self) -> anyhow::Result<()> {
        match std::fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).context("clear the install staging directory"),
        }
    }
}

impl StagedArchive {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    pub fn discard(self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::manifest::test_helpers::{
        archive_with_manifest, write_archive_without_manifest,
    };

    fn staging() -> (tempfile::TempDir, StagingArea) {
        let root = tempfile::tempdir().expect("create temp cache dir");
        let area = StagingArea::new(root.path().join("install-staging"));
        (root, area)
    }

    fn staged_files(area: &StagingArea) -> Vec<std::path::PathBuf> {
        std::fs::read_dir(area.dir())
            .map(|entries| entries.map(|e| e.expect("dir entry").path()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_size_limit_is_sixteen_mebibytes() {
        assert_eq!(MAX_ARCHIVE_BYTES, 16 * 1024 * 1024);
    }

    #[test]
    fn staging_copies_the_archive_and_reads_its_manifest() {
        let (_root, area) = staging();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");

        let staged = area.stage(&archive).expect("staging should succeed");

        assert_eq!(staged.manifest().gadget.id.as_str(), "weather");
        assert!(staged.path().starts_with(area.dir()));
        assert_eq!(
            std::fs::read(staged.path()).expect("read staged copy"),
            std::fs::read(&archive).expect("read source")
        );
    }

    #[test]
    fn an_archive_of_exactly_the_limit_is_accepted() {
        let (_root, area) = staging();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");
        let size = std::fs::metadata(&archive).expect("archive metadata").len();

        let staged = area.with_limit(size).stage(&archive);

        assert!(staged.is_ok(), "{:?}", staged.err());
    }

    #[test]
    fn an_archive_one_byte_over_the_limit_is_rejected_without_leftovers() {
        let (_root, area) = staging();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");
        let size = std::fs::metadata(&archive).expect("archive metadata").len();
        let area = area.with_limit(size - 1);

        let error = area
            .stage(&archive)
            .expect_err("oversized archive must be rejected");

        assert!(format!("{error:#}").contains("larger than"));
        assert!(staged_files(&area).is_empty());
    }

    #[test]
    fn an_invalid_archive_is_rejected_without_leftovers() {
        let (_root, area) = staging();
        let (_src, archive) = write_archive_without_manifest(&[("README.md", b"no manifest")]);

        assert!(area.stage(&archive).is_err());
        assert!(staged_files(&area).is_empty());
    }

    #[test]
    fn a_missing_source_file_is_an_error() {
        let (root, area) = staging();

        assert!(area.stage(&root.path().join("missing.torchsnap")).is_err());
    }

    /// Review and install read the staged copy, so changing the file
    /// the user picked after staging changes nothing.
    #[test]
    fn the_staged_copy_is_independent_of_the_source() {
        let (_root, area) = staging();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");
        let staged = area.stage(&archive).expect("staging should succeed");
        let original = std::fs::read(staged.path()).expect("read staged copy");

        std::fs::write(&archive, b"swapped after review").expect("overwrite source");

        assert_eq!(
            std::fs::read(staged.path()).expect("read staged copy"),
            original
        );
    }

    #[test]
    fn two_stagings_of_the_same_file_get_separate_copies() {
        let (_root, area) = staging();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");

        let first = area.stage(&archive).expect("first staging");
        let second = area.stage(&archive).expect("second staging");

        assert_ne!(first.path(), second.path());
    }

    #[test]
    fn discard_removes_the_staged_copy() {
        let (_root, area) = staging();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");
        let staged = area.stage(&archive).expect("staging should succeed");
        let path = staged.path().to_path_buf();

        staged.discard();

        assert!(!path.exists());
    }

    #[test]
    fn sweep_empties_the_staging_dir_and_nothing_else() {
        let (root, area) = staging();
        let (_src, archive) = archive_with_manifest("weather", "1.4.0", "");
        let _leftover = area.stage(&archive).expect("staging should succeed");
        let neighbour = root.path().join("keep.txt");
        std::fs::write(&neighbour, b"not staging").expect("write neighbour");

        area.sweep().expect("sweep should succeed");

        assert!(staged_files(&area).is_empty());
        assert!(neighbour.exists());
    }

    #[test]
    fn sweep_without_a_staging_dir_does_nothing() {
        let (_root, area) = staging();

        area.sweep().expect("sweep should succeed");
    }
}
