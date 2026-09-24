// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Turning whatever an entry point delivers into archive paths.
//!
//! The settings window sends absolute paths, the OS sends `file://`
//! URLs, and the command line sends paths relative to wherever the
//! user ran it. All of them go through `archive_path`, so the queue
//! only ever sees absolute `*.torchsnap` paths.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Where an install request came from. Shown in the review so the
/// user can tell a file they picked from one another app opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InstallOrigin {
    SettingsPicker,
    SettingsDrop,
    OsOpenFile,
    CommandLine,
}

/// The absolute archive path `input` refers to, or `None` when it is
/// not a `.torchsnap` file or not a path at all (for example an
/// `https://` URL). Relative paths resolve against `cwd`.
pub fn archive_path(input: &OsStr, cwd: &Path) -> Option<PathBuf> {
    let path = match input.to_str() {
        Some(text) if text.contains("://") => {
            let url = url::Url::parse(text).ok()?;
            if url.scheme() != "file" {
                return None;
            }
            url.to_file_path().ok()?
        }
        _ => PathBuf::from(input),
    };

    let is_archive = path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("torchsnap"));
    if !is_archive {
        return None;
    }

    Some(if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn an_absolute_path_is_taken_as_is() {
        assert_eq!(
            archive_path(
                OsStr::new("/Users/me/Downloads/weather.torchsnap"),
                Path::new("/tmp")
            ),
            Some(PathBuf::from("/Users/me/Downloads/weather.torchsnap"))
        );
    }

    #[test]
    fn a_relative_path_resolves_against_the_working_directory() {
        assert_eq!(
            archive_path(
                OsStr::new("gadgets/weather.torchsnap"),
                Path::new("/Users/me")
            ),
            Some(PathBuf::from("/Users/me/gadgets/weather.torchsnap"))
        );
    }

    #[test]
    fn a_file_url_becomes_a_path() {
        assert_eq!(
            archive_path(
                OsStr::new("file:///Users/me/My%20Downloads/weather.torchsnap"),
                Path::new("/tmp")
            ),
            Some(PathBuf::from("/Users/me/My Downloads/weather.torchsnap"))
        );
    }

    #[test]
    fn other_urls_are_not_archives() {
        assert_eq!(
            archive_path(
                OsStr::new("https://example.com/weather.torchsnap"),
                Path::new("/tmp")
            ),
            None
        );
        assert_eq!(
            archive_path(OsStr::new("torchsnap://install?url=x"), Path::new("/tmp")),
            None
        );
    }

    #[test]
    fn only_torchsnap_files_are_accepted() {
        assert_eq!(
            archive_path(OsStr::new("/tmp/notes.txt"), Path::new("/")),
            None
        );
        assert_eq!(
            archive_path(OsStr::new("/tmp/weather"), Path::new("/")),
            None
        );
        assert_eq!(
            archive_path(OsStr::new("/tmp/Weather.TORCHSNAP"), Path::new("/")),
            Some(PathBuf::from("/tmp/Weather.TORCHSNAP"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_is_not_utf8_is_kept_intact() {
        use std::os::unix::ffi::OsStrExt as _;

        let raw = OsStr::from_bytes(b"/tmp/caf\xe9.torchsnap");

        assert_eq!(archive_path(raw, Path::new("/")), Some(PathBuf::from(raw)));
    }

    #[test]
    fn origins_serialize_for_the_frontend() {
        assert_eq!(
            serde_json::to_value(InstallOrigin::SettingsPicker).expect("serializes"),
            "settingsPicker"
        );
        assert_eq!(
            serde_json::from_value::<InstallOrigin>(serde_json::json!("settingsDrop"))
                .expect("deserializes"),
            InstallOrigin::SettingsDrop
        );
    }
}
