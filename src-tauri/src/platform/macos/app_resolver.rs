// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Bundle Resolution (macOS)
//
// Resolves a bundle identifier (e.g. "com.apple.finder") to the
// on-disk location of the installed application via
// LaunchServices. LaunchServices tracks every registered bundle
// by identifier independent of where the user has moved,
// renamed, or reinstalled it, so gadgets can reference an
// application by its stable identifier rather than a path that
// may no longer exist.
// =========================================================

use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSString;

/// Resolve a bundle identifier to the installed application's
/// filesystem path.
///
/// Returns `None` if no application with the given identifier is
/// currently registered with LaunchServices.
pub fn app_path_for_identifier(identifier: &str) -> Option<String> {
    let ns_identifier = NSString::from_str(identifier);
    let workspace = NSWorkspace::sharedWorkspace();
    let url = workspace.URLForApplicationWithBundleIdentifier(&ns_identifier)?;
    let path = url.path()?;
    Some(path.to_string())
}

/// Extract the system-composited icon for the application bundle
/// at `path`.
pub fn icon_image_for_path(path: &str) -> anyhow::Result<Option<image::DynamicImage>> {
    super::cgimage_conversion::nsworkspace_icon_for_file(path)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn app_path_for_identifier_finds_finder() {
        let path = app_path_for_identifier("com.apple.finder")
            .expect("Finder should be installed on this machine");
        assert!(
            path.contains(".app"),
            "path should point at an app bundle: {path}"
        );
    }

    #[test]
    fn app_path_for_identifier_returns_none_for_unknown_bundle() {
        let path = app_path_for_identifier("com.example.definitely-not-installed-abc123");
        assert!(path.is_none());
    }

    #[test]
    fn icon_image_for_path_returns_nonzero_image_for_finder() {
        let path = app_path_for_identifier("com.apple.finder").expect("Finder installed");
        let image = icon_image_for_path(&path)
            .expect("icon extraction should not error")
            .expect("Finder should have an icon");
        assert!(image.width() > 0 && image.height() > 0);
    }
}
