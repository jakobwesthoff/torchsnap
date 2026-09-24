// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether the running copy of Torchsnap can replace itself.
//!
//! The updater replaces the `.app` that contains the running
//! executable (`std::env::current_exe()`, the same path the plugin
//! uses). Two places cannot be updated in a way that helps the user:
//! a copy started straight from the mounted DMG, which is read-only,
//! and a copy macOS runs from a randomized App Translocation path
//! because it was opened from where it was downloaded; replacing that
//! copy would leave the one the user sees untouched.

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocationProblem {
    /// Running from a mounted disk image under `/Volumes/`.
    DiskImage,
    /// Running from an App Translocation path.
    Translocated,
}

pub fn install_location_problem(executable: &Path) -> Option<LocationProblem> {
    if executable
        .components()
        .any(|part| part.as_os_str() == "AppTranslocation")
    {
        return Some(LocationProblem::Translocated);
    }
    if executable.starts_with("/Volumes") {
        return Some(LocationProblem::DiskImage);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applications_folder_is_fine() {
        let exe = Path::new("/Applications/Torchsnap.app/Contents/MacOS/torchsnap");
        assert_eq!(install_location_problem(exe), None);
    }

    #[test]
    fn user_applications_folder_is_fine() {
        let exe = Path::new("/Users/someone/Applications/Torchsnap.app/Contents/MacOS/torchsnap");
        assert_eq!(install_location_problem(exe), None);
    }

    #[test]
    fn mounted_dmg_is_a_disk_image() {
        let exe = Path::new("/Volumes/Torchsnap/Torchsnap.app/Contents/MacOS/torchsnap");
        assert_eq!(
            install_location_problem(exe),
            Some(LocationProblem::DiskImage)
        );
    }

    #[test]
    fn translocated_copy_is_detected() {
        let exe = Path::new(
            "/private/var/folders/xy/abc/T/AppTranslocation/1234-ABCD/d/Torchsnap.app/Contents/MacOS/torchsnap",
        );
        assert_eq!(
            install_location_problem(exe),
            Some(LocationProblem::Translocated)
        );
    }

    #[test]
    fn a_folder_merely_named_like_volumes_is_fine() {
        let exe = Path::new("/Users/someone/Volumes/Torchsnap.app/Contents/MacOS/torchsnap");
        assert_eq!(install_location_problem(exe), None);
    }
}
