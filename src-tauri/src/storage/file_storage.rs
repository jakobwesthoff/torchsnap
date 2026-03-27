// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// File-Based Blob Storage
//
// Generic sharded file storage for binary data. Files are
// stored in a two-level directory hierarchy to avoid large
// flat directories:
//
//   base_dir/<2-hex-prefix>/<full-hash>.<ext>
//
// The `StorageKey` newtype wraps a blake3 hex hash, ensuring
// callers cannot accidentally pass raw strings where a hashed
// key is expected. The `FileStorage` struct handles all I/O
// — creating shard directories, reading, writing, deleting,
// and iterating stored entries.
// =========================================================

use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::{Context, Result};

// =========================================================
// StorageKey
// =========================================================

/// Typesafe storage key — a blake3 hex hash of arbitrary input.
///
/// Constructed via `StorageKey::new(input)`, which hashes the
/// input into a 64-character lowercase hex string. Can be
/// dereferenced to `&str` for comparisons and display.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StorageKey(String);

impl StorageKey {
    /// Hash an arbitrary input string to produce a storage key.
    pub fn new(input: &str) -> Self {
        Self(blake3::hash(input.as_bytes()).to_hex().to_string())
    }

    /// Reconstruct a key from a raw hex string (e.g. parsed from
    /// a filename on disk). The caller is responsible for ensuring
    /// the hex string is a valid blake3 hash.
    pub(crate) fn from_raw(hex: String) -> Self {
        Self(hex)
    }
}

impl Deref for StorageKey {
    type Target = str;

    fn deref(&self) -> &str {
        &self.0
    }
}

// =========================================================
// EntryMetadata
// =========================================================

/// Filesystem metadata for a stored entry.
pub struct EntryMetadata {
    pub modified: SystemTime,
    pub size: u64,
}

// =========================================================
// FileStorage
// =========================================================

/// Sharded file storage for binary blobs.
///
/// Directory layout:
/// ```text
/// base_dir/
///   <first-2-hex-chars>/
///     <full-64-char-hash>.<ext>
/// ```
#[derive(Clone)]
pub struct FileStorage {
    base_dir: PathBuf,
}

impl FileStorage {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    /// Create a sub-storage rooted at `base_dir/<subdir>`.
    ///
    /// Useful for scoping storage per plugin or per content type
    /// without creating a whole new directory tree.
    pub fn scoped(&self, subdir: &str) -> Self {
        Self {
            base_dir: self.base_dir.join(subdir),
        }
    }

    /// Absolute path where an entry with the given key and
    /// extension would be stored.
    pub fn resolve(&self, key: &StorageKey, ext: &str) -> PathBuf {
        let hex: &str = key;
        self.base_dir.join(&hex[..2]).join(format!("{hex}.{ext}"))
    }

    /// Write `data` to the storage location for `key` with
    /// the given file extension, creating shard directories
    /// as needed.
    pub fn store(&self, key: &StorageKey, data: &[u8], ext: &str) -> Result<()> {
        let path = self.resolve(key, ext);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("create storage shard directory")?;
        }
        fs::write(&path, data).with_context(|| format!("write {}", path.display()))
    }

    /// Read the stored data for `key`, returning `Ok(None)` if
    /// no file exists at the expected path.
    pub fn load(&self, key: &StorageKey, ext: &str) -> Result<Option<Vec<u8>>> {
        let path = self.resolve(key, ext);
        match fs::read(&path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e).with_context(|| format!("read {}", path.display())),
        }
    }

    /// Delete the stored file for `key`. No-op if the file
    /// does not exist.
    pub fn delete(&self, key: &StorageKey, ext: &str) -> Result<()> {
        let path = self.resolve(key, ext);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("delete {}", path.display())),
        }
    }

    /// Check whether a file exists for the given key and extension.
    pub fn exists(&self, key: &StorageKey, ext: &str) -> bool {
        self.resolve(key, ext).exists()
    }

    /// Return filesystem metadata for a stored entry, or `None`
    /// if the file does not exist.
    pub fn metadata(&self, key: &StorageKey, ext: &str) -> Option<EntryMetadata> {
        let meta = fs::metadata(self.resolve(key, ext)).ok()?;
        Some(EntryMetadata {
            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
            size: meta.len(),
        })
    }

    /// Iterate all stored entries, yielding `(key, extension, metadata)`
    /// for each file found in the shard directories.
    ///
    /// Silently skips files whose names cannot be parsed as valid
    /// entries (e.g. no extension, non-UTF-8 names).
    pub fn entries(&self) -> impl Iterator<Item = (StorageKey, String, EntryMetadata)> {
        let shard_dirs = fs::read_dir(&self.base_dir).into_iter().flatten().flatten();

        shard_dirs.flat_map(|shard_entry| {
            let shard_path = shard_entry.path();
            if !shard_path.is_dir() {
                return Vec::new();
            }

            let files = match fs::read_dir(&shard_path) {
                Ok(f) => f,
                Err(_) => return Vec::new(),
            };

            files
                .flatten()
                .filter_map(|file_entry| {
                    let path = file_entry.path();
                    let stem = path.file_stem()?.to_str()?.to_string();
                    let ext = path.extension()?.to_str()?.to_string();
                    let meta = fs::metadata(&path).ok()?;

                    Some((
                        StorageKey::from_raw(stem),
                        ext,
                        EntryMetadata {
                            modified: meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                            size: meta.len(),
                        },
                    ))
                })
                .collect::<Vec<_>>()
        })
    }
}
