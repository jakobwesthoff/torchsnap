// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Discovery
//
// Enumerates the search roots the host scans at startup and
// lists the plugin entries (archive files and plugin
// directories) within each root.
//
// Three roots are considered, in precedence order:
//
// 1. **System** — `<resource_dir>/plugins/`, populated by
//    the Tauri bundler from the `target/bundled-plugins/`
//    staging dir. Only plugins listed in
//    `plugins/bundled.toml` end up here.
// 2. **Dev** — `<CARGO_MANIFEST_DIR>/../plugins/` in debug
//    builds. Tags plugins `PluginSourceKind::Dev` so the
//    Plugins settings panel can badge them accordingly.
//    Completely elided from release builds via
//    `cfg(debug_assertions)`.
// 3. **User** — `<app_data_dir>/plugins/`, where
//    user-installed `.torchsnap` archives (and optional
//    directory-form plugins for development) live.
//
// Per-root, archive entries win over a sibling directory
// with the same stem — so `calculator.torchsnap` shadows
// `calculator/` if both exist in the same root. Collisions
// *across* roots are handled by the caller
// (`load_wasm_plugins` in `lib.rs`): the earlier root wins
// and a warning is logged.
// =========================================================

use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::source::PluginSourceKind;

/// Build the ordered list of `(kind, path)` roots the loader
/// should scan. Caller passes the resolved `resource_dir` and
/// `app_data_dir`; the dev path is inlined from
/// `CARGO_MANIFEST_DIR` and only appears in debug builds.
///
/// Missing directories are simply omitted — a release build
/// without any user plugins yet, or a dev checkout without a
/// sibling `plugins/` directory, both produce a non-empty but
/// possibly shorter root list. The caller treats an empty
/// root list as "no plugins to load" rather than an error.
pub fn enumerate_search_roots(
    resource_dir: Option<&Path>,
    app_data_dir: &Path,
) -> Vec<(PluginSourceKind, PathBuf)> {
    let mut roots: Vec<(PluginSourceKind, PathBuf)> = Vec::new();

    // System: plugins bundled into the app's resources at build
    // time. Release builds always expose a resource_dir; debug
    // builds may not have one, in which case System is absent.
    if let Some(res) = resource_dir {
        let res_plugins = res.join("plugins");
        if res_plugins.is_dir() {
            roots.push((PluginSourceKind::System, res_plugins));
        }
    }

    // Dev: repo-relative plugin sources. Debug builds only —
    // `#[cfg(debug_assertions)]` strips this branch entirely
    // from release artifacts so a stray path at a production
    // user's CARGO_MANIFEST_DIR could never accidentally
    // activate.
    #[cfg(debug_assertions)]
    {
        let dev_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../plugins");
        if dev_dir.is_dir() {
            roots.push((PluginSourceKind::Dev, dev_dir));
        }
    }

    // User: install target for `.torchsnap` files dropped via
    // the Plugins settings panel. Always scanned (when the
    // directory exists); users can have plugins regardless of
    // build profile.
    let user_plugins = app_data_dir.join("plugins");
    if user_plugins.is_dir() {
        roots.push((PluginSourceKind::User, user_plugins));
    }

    roots
}

/// List every plugin entry inside a single root, applying the
/// per-root archive-over-directory precedence rule. Returns
/// `Ok(Vec::new())` when the root does not exist or is not
/// readable — a missing root is not an error.
///
/// An entry is one of:
///
/// - A `.torchsnap` archive file at the root (any extension
///   other than `.torchsnap` is skipped).
/// - A directory at the root containing a `manifest.toml` at
///   its top level (plain directories without a manifest are
///   not plugins and are skipped).
///
/// When both forms coexist (`foo.torchsnap` alongside
/// `foo/`), the archive is kept and the directory is dropped.
pub fn scan_plugin_entries(root: &Path) -> Vec<PathBuf> {
    let read_dir = match std::fs::read_dir(root) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    // First pass: collect all entries so we can do the
    // archive-over-directory precedence check in the second
    // pass. A single read_dir iteration would work if we
    // remembered seen names, but the two-pass shape is easier
    // to reason about and read_dir results are already
    // materialized into Vec for precedence lookups anyway.
    let all_entries: Vec<PathBuf> = read_dir.flatten().map(|entry| entry.path()).collect();

    // Track archive stems so the directory pass can skip
    // `foo/` when `foo.torchsnap` is present in the same root.
    let mut archive_stems: HashSet<OsString> = HashSet::new();
    let mut entries: Vec<PathBuf> = Vec::new();

    for path in &all_entries {
        if path.extension().is_some_and(|ext| ext == "torchsnap") && path.is_file() {
            if let Some(stem) = path.file_stem() {
                archive_stems.insert(stem.to_os_string());
            }
            entries.push(path.clone());
        }
    }

    for path in &all_entries {
        if !path.is_dir() {
            continue;
        }
        if !path.join("manifest.toml").exists() {
            continue;
        }
        let name = match path.file_name() {
            Some(n) => n.to_os_string(),
            None => continue,
        };
        if archive_stems.contains(&name) {
            continue;
        }
        entries.push(path.clone());
    }

    entries
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Touch a file with minimal zero-byte content. Enough
    /// to register as a `.torchsnap` for scanning — these
    /// tests don't open the archive, only list it.
    fn touch(path: &Path) {
        std::fs::write(path, &[]).expect("write stub file");
    }

    /// Create a directory-form plugin with a minimal
    /// manifest.toml so the scanner recognizes it.
    fn make_dir_plugin(root: &Path, name: &str) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        std::fs::write(dir.join("manifest.toml"), "[plugin]\n").expect("write manifest");
    }

    #[test]
    fn scan_returns_empty_when_root_is_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("does-not-exist");
        assert!(scan_plugin_entries(&missing).is_empty());
    }

    #[test]
    fn scan_returns_empty_when_root_is_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(scan_plugin_entries(tmp.path()).is_empty());
    }

    #[test]
    fn scan_finds_archive_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(&tmp.path().join("alpha.torchsnap"));
        touch(&tmp.path().join("beta.torchsnap"));

        let entries = scan_plugin_entries(tmp.path());
        let names: Vec<String> = entries
            .iter()
            .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"alpha.torchsnap".to_string()));
        assert!(names.contains(&"beta.torchsnap".to_string()));
    }

    #[test]
    fn scan_finds_directory_plugins() {
        let tmp = tempfile::tempdir().expect("tempdir");
        make_dir_plugin(tmp.path(), "alpha");
        make_dir_plugin(tmp.path(), "beta");

        let entries = scan_plugin_entries(tmp.path());
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn scan_skips_directories_without_manifest() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("not-a-plugin")).expect("mkdir");
        assert!(scan_plugin_entries(tmp.path()).is_empty());
    }

    #[test]
    fn scan_skips_unrelated_files() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(&tmp.path().join("README.md"));
        touch(&tmp.path().join("notes.txt"));
        assert!(scan_plugin_entries(tmp.path()).is_empty());
    }

    /// The precedence rule exists so a developer can keep a
    /// working-tree copy (`foo/`) alongside a published
    /// artifact (`foo.torchsnap`) and have the artifact win.
    /// A collision in the other direction would silently load
    /// stale code.
    #[test]
    fn archive_wins_over_sibling_directory_with_same_stem() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(&tmp.path().join("calculator.torchsnap"));
        make_dir_plugin(tmp.path(), "calculator");

        let entries = scan_plugin_entries(tmp.path());
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].file_name().and_then(|n| n.to_str()),
            Some("calculator.torchsnap")
        );
    }

    /// An archive whose stem does *not* match any sibling
    /// directory leaves other directory plugins untouched.
    #[test]
    fn archive_without_matching_directory_does_not_block_other_dirs() {
        let tmp = tempfile::tempdir().expect("tempdir");
        touch(&tmp.path().join("archive-only.torchsnap"));
        make_dir_plugin(tmp.path(), "dir-only");

        let entries = scan_plugin_entries(tmp.path());
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn enumerate_returns_empty_when_no_roots_exist() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let app_data = tmp.path().join("app-data");
        std::fs::create_dir_all(&app_data).expect("mkdir");
        // No plugins/ subdir under app_data; no resource dir.
        let roots = enumerate_search_roots(None, &app_data);
        // In debug builds the dev root may still resolve if the
        // test is run from the torchsnap workspace, so accept
        // either 0 or 1 (Dev) roots here.
        assert!(
            roots.is_empty() || roots.iter().all(|(k, _)| *k == PluginSourceKind::Dev),
            "unexpected non-dev root: {roots:?}"
        );
    }

    #[test]
    fn enumerate_includes_system_when_resource_plugins_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let resource = tmp.path().join("res");
        let resource_plugins = resource.join("plugins");
        std::fs::create_dir_all(&resource_plugins).expect("mkdir");
        let app_data = tmp.path().join("app-data");
        std::fs::create_dir_all(&app_data).expect("mkdir");

        let roots = enumerate_search_roots(Some(&resource), &app_data);
        let kinds: Vec<PluginSourceKind> = roots.iter().map(|(k, _)| *k).collect();
        assert!(
            kinds.contains(&PluginSourceKind::System),
            "System root missing from {roots:?}"
        );
    }

    #[test]
    fn enumerate_includes_user_when_app_data_plugins_exists() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let app_data = tmp.path().join("app-data");
        std::fs::create_dir_all(app_data.join("plugins")).expect("mkdir");

        let roots = enumerate_search_roots(None, &app_data);
        let kinds: Vec<PluginSourceKind> = roots.iter().map(|(k, _)| *k).collect();
        assert!(
            kinds.contains(&PluginSourceKind::User),
            "User root missing from {roots:?}"
        );
    }

    /// Precedence of the returned roots is load-bearing for
    /// collision handling: the loader processes them in order
    /// and the *first* occurrence of a plugin id wins.
    /// System must therefore precede Dev and User when all
    /// three are present.
    #[test]
    fn enumerate_orders_system_before_user() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let resource = tmp.path().join("res");
        std::fs::create_dir_all(resource.join("plugins")).expect("mkdir");
        let app_data = tmp.path().join("app-data");
        std::fs::create_dir_all(app_data.join("plugins")).expect("mkdir");

        let roots = enumerate_search_roots(Some(&resource), &app_data);
        let system_idx = roots
            .iter()
            .position(|(k, _)| *k == PluginSourceKind::System);
        let user_idx = roots.iter().position(|(k, _)| *k == PluginSourceKind::User);
        assert!(system_idx.is_some() && user_idx.is_some());
        assert!(system_idx < user_idx, "System must precede User");
    }

    #[test]
    fn enumerate_skips_system_when_resource_dir_lacks_plugins_subdir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let resource = tmp.path().join("res");
        std::fs::create_dir_all(&resource).expect("mkdir");
        // resource/plugins is intentionally absent.
        let app_data = tmp.path().join("app-data");
        std::fs::create_dir_all(&app_data).expect("mkdir");

        let roots = enumerate_search_roots(Some(&resource), &app_data);
        assert!(
            roots.iter().all(|(k, _)| *k != PluginSourceKind::System),
            "System should not be included when res/plugins is missing"
        );
    }
}
