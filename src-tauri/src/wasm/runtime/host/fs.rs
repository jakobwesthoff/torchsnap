// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// FS host import — thin bridge
//
// Delegates to `FilesystemCap` for permission checking and
// I/O. This module provides only the `From` impls between
// native cap types and WIT types, plus the `Host` trait impl
// that routes through the capability.
// =========================================================

use crate::caps::{FileMetadata, FilesystemError};
use crate::wasm::bindings;

use super::super::GadgetState;

// =========================================================
// From impls: Cap → WIT (outputs + errors)
// =========================================================

impl From<FilesystemError> for bindings::torchsnap::gadget::fs::FsError {
    fn from(e: FilesystemError) -> Self {
        use bindings::torchsnap::gadget::fs::FsError;
        match e {
            FilesystemError::PermissionDenied(msg) => FsError::PermissionDenied(msg),
            FilesystemError::InvalidPath(msg) => FsError::InvalidPath(msg),
            FilesystemError::NotFound => FsError::NotFound,
            FilesystemError::Io(msg) => FsError::Io(msg),
        }
    }
}

impl From<FileMetadata> for bindings::torchsnap::gadget::fs::FileMetadata {
    fn from(m: FileMetadata) -> Self {
        Self {
            size: m.size,
            modified_unix_ms: m.modified_unix_ms,
            is_symlink: m.is_symlink,
        }
    }
}

// =========================================================
// Host trait impl
// =========================================================

impl bindings::torchsnap::gadget::fs::Host for GadgetState {
    fn read_file(
        &mut self,
        path: String,
    ) -> Result<Vec<u8>, bindings::torchsnap::gadget::fs::FsError> {
        let filesystem = self.caps().filesystem.as_ref().ok_or_else(|| {
            bindings::torchsnap::gadget::fs::FsError::PermissionDenied(path.clone())
        })?;

        filesystem
            .read_file(&path)
            .map_err(bindings::torchsnap::gadget::fs::FsError::from)
    }

    fn file_exists(&mut self, path: String) -> bool {
        let Some(filesystem) = self.caps().filesystem.as_ref() else {
            return false;
        };

        filesystem.file_exists(&path)
    }

    fn metadata(
        &mut self,
        path: String,
    ) -> Result<
        bindings::torchsnap::gadget::fs::FileMetadata,
        bindings::torchsnap::gadget::fs::FsError,
    > {
        let filesystem = self.caps().filesystem.as_ref().ok_or_else(|| {
            bindings::torchsnap::gadget::fs::FsError::PermissionDenied(path.clone())
        })?;

        filesystem
            .metadata(&path)
            .map(bindings::torchsnap::gadget::fs::FileMetadata::from)
            .map_err(bindings::torchsnap::gadget::fs::FsError::from)
    }
}
