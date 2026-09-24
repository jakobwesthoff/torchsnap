// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Filesystem locations used by gadget install and uninstall.
//!
//! Built once in `setup` from the app data dir and managed as Tauri
//! state, so install code never resolves paths through an
//! `AppHandle` and tests can point everything at a temp dir.

use std::path::{Path, PathBuf};

pub const UNINSTALL_MARKER_SUFFIX: &str = ".uninstall";
pub const BACKUP_SUFFIX: &str = ".torchsnap.prev";

#[derive(Debug, Clone)]
pub struct InstallPaths {
    /// `<app_data_dir>/gadgets/`, the user gadget root that startup
    /// discovery scans.
    pub gadgets_dir: PathBuf,
    /// `<app_data_dir>/gadget-home/`, one state tree per gadget id.
    pub gadget_home_dir: PathBuf,
}

impl InstallPaths {
    pub fn new(app_data_dir: &Path) -> Self {
        Self {
            gadgets_dir: app_data_dir.join("gadgets"),
            gadget_home_dir: app_data_dir.join("gadget-home"),
        }
    }

    /// The archive form of a user gadget, `gadgets/<id>.torchsnap`.
    pub fn archive(&self, gadget_id: &str) -> PathBuf {
        self.gadgets_dir.join(format!("{gadget_id}.torchsnap"))
    }

    /// The directory form of a user gadget, `gadgets/<id>/`. Install
    /// never creates it; it exists only when placed there by hand.
    pub fn directory(&self, gadget_id: &str) -> PathBuf {
        self.gadgets_dir.join(gadget_id)
    }

    /// The gadget's state tree, `gadget-home/<id>/`.
    pub fn home(&self, gadget_id: &str) -> PathBuf {
        self.gadget_home_dir.join(gadget_id)
    }

    /// Copy of the archive that was replaced in this session,
    /// `gadgets/.<id>.torchsnap.prev`, kept so the replace can be
    /// undone. The leading dot keeps it out of startup discovery.
    pub fn backup(&self, gadget_id: &str) -> PathBuf {
        self.gadgets_dir
            .join(format!(".{gadget_id}{BACKUP_SUFFIX}"))
    }

    /// Marker left by uninstall, `gadgets/.<id>.uninstall`. The leading
    /// dot keeps it out of startup discovery.
    pub fn uninstall_marker(&self, gadget_id: &str) -> PathBuf {
        self.gadgets_dir
            .join(format!(".{gadget_id}{UNINSTALL_MARKER_SUFFIX}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_derive_from_the_app_data_dir() {
        let paths = InstallPaths::new(Path::new("/data"));

        assert_eq!(
            paths.archive("weather"),
            Path::new("/data/gadgets/weather.torchsnap")
        );
        assert_eq!(
            paths.directory("weather"),
            Path::new("/data/gadgets/weather")
        );
        assert_eq!(
            paths.home("weather"),
            Path::new("/data/gadget-home/weather")
        );
        assert_eq!(
            paths.backup("weather"),
            Path::new("/data/gadgets/.weather.torchsnap.prev")
        );
        assert_eq!(
            paths.uninstall_marker("weather"),
            Path::new("/data/gadgets/.weather.uninstall")
        );
    }
}
