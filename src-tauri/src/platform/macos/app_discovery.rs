// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Discovery (macOS)
//
// Discovers installed applications by querying the Spotlight
// index via the `mdfind` CLI and reading each app bundle's
// `Info.plist` for its display name and bundle identifier.
//
// Apps that set `LSUIElement` or `LSBackgroundOnly`, such as
// menu-bar apps, stay in the list. Users launch them on
// purpose, for example Alfred, Bartender or Screenshot.
//
// Spotlight also returns bundles nested inside other apps. A
// nested bundle stays when it sits directly in a folder named
// `Applications`, as Xcode's tools do in
// `Contents/Applications/`. Any other nested bundle, such as a
// login item or helper, is a part of its host app and is left
// out.
//
// The Mdfind struct encapsulates the raw subprocess call so
// the discovery logic stays focused on metadata extraction.
// =========================================================

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;
use image::DynamicImage;

use super::cgimage_conversion::nsworkspace_icon_for_file;
use crate::platform::app_discovery::{AppDiscovery, DiscoveredApp};

/// Thin wrapper around the macOS `mdfind` Spotlight CLI.
struct Mdfind;

impl Mdfind {
    /// Run a Spotlight query and return one path per result line.
    fn query(predicate: &str) -> anyhow::Result<Vec<PathBuf>> {
        let output = Command::new("mdfind")
            .arg(predicate)
            .output()
            .context("spawn mdfind process")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("mdfind exited with {}: {stderr}", output.status);
        }

        let stdout = String::from_utf8(output.stdout).context("decode mdfind output as UTF-8")?;

        Ok(stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect())
    }
}

/// Whether an app path has an `Applications` folder anywhere above
/// the bundle.
///
/// This covers `/Applications/`, `/System/Applications/`,
/// `~/Applications/` and `/System/Library/CoreServices/Applications/`
/// (Keychain Access). It also admits the `Contents/Applications/`
/// folder of any app, which lists Finder's AirDrop although Finder
/// itself sits outside an Applications folder.
fn is_in_applications_dir(path: &Path) -> bool {
    path.to_string_lossy().contains("/Applications/")
}

/// Whether a bundle is embedded in another app as one of its parts.
///
/// Each `.app` bundle below the outermost one must sit directly in
/// a folder named `Applications`. A login item in
/// `Contents/Library/LoginItems/` or a helper in
/// `Contents/Frameworks/` fails that, and so does any app nested
/// inside such a helper.
///
/// `path` ends in the bundle's own `.app` component, as the
/// Spotlight content-type query returns.
fn is_embedded_helper(path: &Path) -> bool {
    let components: Vec<&OsStr> = path.components().map(|c| c.as_os_str()).collect();
    let bundle_indices: Vec<usize> = components
        .iter()
        .enumerate()
        .filter(|(_, name)| Path::new(name).extension() == Some(OsStr::new("app")))
        .map(|(index, _)| index)
        .collect();

    bundle_indices.windows(2).any(|pair| {
        let (outer, inner) = (pair[0], pair[1]);
        let parent = inner - 1;
        parent == outer || components[parent] != "Applications"
    })
}

/// Read an app bundle's `Contents/Info.plist` and build its
/// `DiscoveredApp`. Whether the bundle is listed at all is decided
/// by `apps_from_bundle_paths`.
fn try_discover_app(path: &Path) -> anyhow::Result<DiscoveredApp> {
    let plist_path = path.join("Contents/Info.plist");
    let info: plist::Dictionary = plist::from_file(&plist_path).context("read Info.plist")?;

    // Display name resolution order:
    //   1. CFBundleDisplayName: the localized user-facing name
    //   2. CFBundleName: shorter internal name
    //   3. Filename minus .app: last resort fallback
    let name = info
        .get("CFBundleDisplayName")
        .and_then(|v| v.as_string())
        .or_else(|| info.get("CFBundleName").and_then(|v| v.as_string()))
        .map(String::from)
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        });

    let bundle_id = info
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .map(String::from);

    let id = path.to_string_lossy().into_owned();

    Ok(DiscoveredApp {
        id,
        name,
        path: path.to_owned(),
        bundle_id,
        icon_path: None,
    })
}

/// Build the app list from the bundle paths Spotlight returned.
fn apps_from_bundle_paths(paths: &[PathBuf]) -> Vec<DiscoveredApp> {
    let mut apps = Vec::with_capacity(paths.len());

    for path in paths {
        // Only index apps in known application directories.
        // This skips system agents in /System/Library/CoreServices/,
        // framework helpers, and other non-user-facing bundles.
        if !is_in_applications_dir(path) {
            continue;
        }

        // A bundle nested in another app outside an Applications
        // folder is a part of that app, such as a login item or
        // helper.
        if is_embedded_helper(path) {
            continue;
        }

        match try_discover_app(path) {
            Ok(app) => apps.push(app),
            // Individual parse failures should not abort the
            // entire discovery. Log and continue.
            Err(e) => {
                eprintln!("skipping {}: {e:#}", path.display());
            }
        }
    }

    apps
}

/// Discovers macOS applications via Spotlight (`mdfind`) and
/// `Info.plist` metadata parsing.
pub struct MdfindDiscovery;

impl AppDiscovery for MdfindDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>> {
        let paths = Mdfind::query("kMDItemContentType == 'com.apple.application-bundle'")
            .context("query Spotlight for application bundles")?;

        Ok(apps_from_bundle_paths(&paths))
    }

    fn icon(&self, app: &DiscoveredApp) -> anyhow::Result<Option<DynamicImage>> {
        nsworkspace_icon_for_file(&app.path.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================
    // Bundle fixtures
    //
    // Each test builds bundles in a temporary directory. Its path
    // lies under /var/folders, which has no `Applications` folder,
    // so a fixture passes the folder check only when the test puts
    // it below an `Applications` folder of its own.
    // =========================================================

    /// Create `<root>/<relative>/Contents/Info.plist` holding `info`
    /// and return the bundle path.
    fn write_bundle(root: &Path, relative: &str, info: plist::Dictionary) -> PathBuf {
        let bundle = root.join(relative);
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(&contents).expect("temporary directory is writable");
        plist::to_file_xml(contents.join("Info.plist"), &plist::Value::Dictionary(info))
            .expect("temporary directory is writable");
        bundle
    }

    fn plist_with_name(name: &str) -> plist::Dictionary {
        let mut info = plist::Dictionary::new();
        info.insert("CFBundleName".into(), name.into());
        info
    }

    fn names(apps: &[DiscoveredApp]) -> Vec<&str> {
        apps.iter().map(|app| app.name.as_str()).collect()
    }

    #[test]
    fn a_top_level_app_is_listed_with_its_plist_metadata() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let mut info = plist_with_name("Safari");
        info.insert("CFBundleDisplayName".into(), "Safari Display".into());
        info.insert("CFBundleIdentifier".into(), "com.apple.Safari".into());
        let bundle = write_bundle(root.path(), "Applications/Safari.app", info);

        let apps = apps_from_bundle_paths(std::slice::from_ref(&bundle));

        assert_eq!(apps.len(), 1);
        let app = &apps[0];
        assert_eq!(app.id, bundle.to_string_lossy());
        assert_eq!(app.name, "Safari Display");
        assert_eq!(app.path, bundle);
        assert_eq!(app.bundle_id.as_deref(), Some("com.apple.Safari"));
        assert_eq!(app.icon_path, None);
    }

    #[test]
    fn the_name_falls_back_to_cfbundlename() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let bundle = write_bundle(
            root.path(),
            "Applications/Notes.app",
            plist_with_name("Notes Name"),
        );

        let apps = apps_from_bundle_paths(&[bundle]);

        assert_eq!(names(&apps), ["Notes Name"]);
        assert_eq!(apps[0].bundle_id, None);
    }

    #[test]
    fn the_name_falls_back_to_the_file_stem() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let bundle = write_bundle(
            root.path(),
            "Applications/Plain Tool.app",
            plist::Dictionary::new(),
        );

        let apps = apps_from_bundle_paths(&[bundle]);

        assert_eq!(names(&apps), ["Plain Tool"]);
    }

    #[test]
    fn a_bundle_outside_an_applications_folder_is_skipped() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let bundle = write_bundle(root.path(), "Downloads/Stray.app", plist_with_name("Stray"));

        let apps = apps_from_bundle_paths(&[bundle]);

        assert!(apps.is_empty(), "listed: {:?}", names(&apps));
    }

    #[test]
    fn a_bundle_without_a_readable_plist_is_skipped() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let broken = root.path().join("Applications/Broken.app");
        std::fs::create_dir_all(&broken).expect("temporary directory is writable");
        let valid = write_bundle(
            root.path(),
            "Applications/Valid.app",
            plist_with_name("Valid"),
        );

        let apps = apps_from_bundle_paths(&[broken, valid]);

        assert_eq!(names(&apps), ["Valid"]);
    }

    #[test]
    fn menu_bar_and_background_apps_stay_listed() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let mut menu_bar = plist_with_name("Menu Bar");
        menu_bar.insert("LSUIElement".into(), true.into());
        let mut menu_bar_string = plist_with_name("Menu Bar String");
        menu_bar_string.insert("LSUIElement".into(), "1".into());
        let mut background = plist_with_name("Background");
        background.insert("LSBackgroundOnly".into(), true.into());
        let bundles = [
            write_bundle(root.path(), "Applications/Menu Bar.app", menu_bar),
            write_bundle(
                root.path(),
                "Applications/Menu Bar String.app",
                menu_bar_string,
            ),
            write_bundle(root.path(), "Applications/Background.app", background),
        ];

        let apps = apps_from_bundle_paths(&bundles);

        assert_eq!(names(&apps), ["Menu Bar", "Menu Bar String", "Background"]);
    }

    #[test]
    fn an_app_in_contents_applications_of_a_bundle_outside_applications_stays_listed() {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let finder = write_bundle(
            root.path(),
            "CoreServices/Finder.app",
            plist_with_name("Finder"),
        );
        let airdrop = write_bundle(
            &finder,
            "Contents/Applications/AirDrop.app",
            plist_with_name("AirDrop"),
        );

        let apps = apps_from_bundle_paths(&[finder, airdrop]);

        assert_eq!(names(&apps), ["AirDrop"]);
    }

    // =========================================================
    // Bundles nested in other apps
    // =========================================================

    /// Build a chain of nested bundles, each path relative to the
    /// bundle before it (the first relative to a fresh temporary
    /// directory), name each after its file stem, and return the
    /// names `apps_from_bundle_paths` lists.
    fn listed_names_of_chain(chain: &[&str]) -> Vec<String> {
        let root = tempfile::tempdir().expect("temporary directory can be created");
        let mut parent = root.path().to_owned();
        let mut bundles = Vec::new();
        for relative in chain {
            let stem = Path::new(relative)
                .file_stem()
                .expect("chain entries name a bundle")
                .to_string_lossy()
                .into_owned();
            let bundle = write_bundle(&parent, relative, plist_with_name(&stem));
            bundles.push(bundle.clone());
            parent = bundle;
        }

        apps_from_bundle_paths(&bundles)
            .into_iter()
            .map(|app| app.name)
            .collect()
    }

    #[test]
    fn a_login_item_inside_an_app_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Library/LoginItems/Foo Login.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn a_helper_inside_a_framework_in_an_app_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Frameworks/Foo Framework.framework/Versions/A/Helpers/Foo Helper.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn an_app_in_a_subfolder_of_contents_applications_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Applications/Extras/Bar.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn an_app_inside_a_helper_is_skipped() {
        let listed = listed_names_of_chain(&[
            "Applications/Foo.app",
            "Contents/Library/LoginItems/Bar.app",
            "Contents/Applications/Baz.app",
        ]);

        assert_eq!(listed, ["Foo"], "helper listed");
    }

    #[test]
    fn an_app_in_contents_applications_stays_listed() {
        let listed = listed_names_of_chain(&[
            "Applications/Xcode.app",
            "Contents/Applications/Instruments.app",
        ]);

        assert_eq!(listed, ["Xcode", "Instruments"]);
    }

    #[test]
    fn an_app_in_contents_developer_applications_stays_listed() {
        let listed = listed_names_of_chain(&[
            "Applications/Xcode.app",
            "Contents/Developer/Applications/Simulator.app",
        ]);

        assert_eq!(listed, ["Xcode", "Simulator"]);
    }

    #[test]
    fn an_app_nested_through_two_applications_folders_stays_listed() {
        let listed = listed_names_of_chain(&[
            "Applications/Outer.app",
            "Contents/Applications/Middle.app",
            "Contents/Applications/Inner.app",
        ]);

        assert_eq!(listed, ["Outer", "Middle", "Inner"]);
    }

    #[test]
    fn a_top_level_app_is_no_embedded_helper() {
        assert!(!is_embedded_helper(Path::new("/Applications/Safari.app")));
    }

    #[test]
    fn a_folder_merely_named_like_an_app_is_no_host_bundle() {
        assert!(!is_embedded_helper(Path::new(
            "/Applications/Old.app.backup/Bar.app"
        )));
    }

    #[test]
    fn an_app_directly_in_contents_is_an_embedded_helper() {
        assert!(is_embedded_helper(Path::new(
            "/Applications/Foo.app/Contents/Bar.app"
        )));
    }

    #[test]
    fn an_app_directly_in_an_applications_folder_of_an_app_is_no_embedded_helper() {
        assert!(!is_embedded_helper(Path::new(
            "/Applications/Foo.app/Applications/Bar.app"
        )));
    }

    #[test]
    fn an_app_below_a_helper_is_an_embedded_helper() {
        assert!(is_embedded_helper(Path::new(
            "/Applications/Foo.app/Contents/Library/LoginItems/Bar.app/Contents/Applications/Baz.app"
        )));
    }

    #[test]
    fn a_helper_with_a_non_utf8_name_is_an_embedded_helper() {
        use std::os::unix::ffi::OsStrExt;

        let path = Path::new("/Applications/Foo.app/Contents/Library/LoginItems")
            .join(OsStr::from_bytes(b"\xff.app"));

        assert!(is_embedded_helper(&path));
    }
}
