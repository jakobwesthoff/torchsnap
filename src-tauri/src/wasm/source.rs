// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Source Abstraction
//
// Plugins can be loaded from two kinds of backing stores:
//
// - `DirectorySource` — a plain directory on disk, used
//   during development to avoid re-zipping on every change.
// - `ArchiveSource`   — a `.torchsnap` zip archive, used
//   in production.
//
// Both implement `PluginSource`, which provides access to
// the parsed manifest and the raw bytes of any file within
// the plugin. The rest of the plugin system is agnostic to
// which source loaded the plugin.
// =========================================================

use std::path::{Path, PathBuf};

use anyhow::Context as _;

use super::manifest::Manifest;

// =========================================================
// PluginSource Trait
// =========================================================

/// A read-only view into a plugin's files, regardless of
/// whether they come from a directory or a zip archive.
pub trait PluginSource: Send + Sync {
    /// The parsed manifest for this plugin.
    fn manifest(&self) -> &Manifest;

    /// Read the raw bytes of a file within the plugin.
    ///
    /// The `path` is relative to the plugin root (matching
    /// the paths used in `manifest.toml`).
    fn read_file(&self, path: &str) -> anyhow::Result<Vec<u8>>;

    /// Read the WASM component binary.
    ///
    /// Convenience wrapper around `read_file` using the
    /// `wasm` path from the manifest.
    fn read_wasm(&self) -> anyhow::Result<Vec<u8>> {
        self.read_file(&self.manifest().plugin.wasm)
    }
}

// =========================================================
// DirectorySource
// =========================================================

/// Loads a plugin from a plain directory on disk.
///
/// The directory must contain a `manifest.toml` at its root.
/// All file paths in the manifest are resolved relative to
/// this directory.
pub struct DirectorySource {
    root: PathBuf,
    manifest: Manifest,
}

impl DirectorySource {
    /// Open a plugin directory and parse its manifest.
    pub fn open(root: impl Into<PathBuf>) -> anyhow::Result<Self> {
        let root = root.into();
        let manifest_path = root.join("manifest.toml");

        let toml_source = std::fs::read_to_string(&manifest_path).with_context(|| {
            format!(
                "reading manifest at {}",
                manifest_path.display()
            )
        })?;

        let manifest = Manifest::parse(&toml_source).with_context(|| {
            format!(
                "parsing manifest at {}",
                manifest_path.display()
            )
        })?;

        Ok(Self { root, manifest })
    }

    /// The root directory this source reads from.
    pub fn root(&self) -> &Path {
        &self.root
    }

}

impl PluginSource for DirectorySource {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn read_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let full_path = self.root.join(path);

        // Prevent path traversal outside the plugin directory.
        // We canonicalize both the root and the target path to
        // resolve symlinks and `..` components, then verify that
        // the target is still within the root.
        let canonical_root = self
            .root
            .canonicalize()
            .context("resolving plugin root directory")?;

        let canonical = match full_path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // If canonicalize fails (e.g., file doesn't exist),
                // normalize manually to check for traversal before
                // returning the more specific "file not found" error.
                let normalized = normalize_path(&full_path);
                anyhow::ensure!(
                    normalized.starts_with(&canonical_root),
                    "plugin file path `{path}` escapes the plugin directory"
                );
                // Path is within bounds but file doesn't exist.
                return std::fs::read(&full_path)
                    .with_context(|| format!("reading plugin file `{path}`"));
            }
        };

        anyhow::ensure!(
            canonical.starts_with(&canonical_root),
            "plugin file path `{path}` escapes the plugin directory"
        );

        std::fs::read(&canonical)
            .with_context(|| format!("reading plugin file `{path}`"))
    }
}

// =========================================================
// Path Normalization
// =========================================================

/// Normalize a path by resolving `.` and `..` components
/// lexically (without hitting the filesystem). This is used
/// as a fallback when `canonicalize()` fails because the
/// target file doesn't exist — we still need to detect
/// traversal attempts.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            other => result.push(other),
        }
    }
    result
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a temporary plugin directory with a
    /// manifest and optional extra files.
    fn make_plugin_dir(
        manifest_toml: &str,
        files: &[(&str, &[u8])],
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path().to_path_buf();

        std::fs::write(root.join("manifest.toml"), manifest_toml)
            .expect("write manifest");

        for (path, contents) in files {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).expect("create parent dirs");
            }
            std::fs::write(&full, contents).expect("write file");
        }

        (dir, root)
    }

    const MINIMAL_MANIFEST: &str = r#"
        [plugin]
        id = "test-plugin"
        name = "Test Plugin"
        description = "A test plugin"
        version = "0.1.0"
        wasm = "plugin.wasm"
        icon = "heroicons:beaker"
    "#;

    #[test]
    fn open_valid_directory() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"fake wasm bytes")],
        );

        let source = DirectorySource::open(&root).expect("should open");
        assert_eq!(source.manifest().plugin.id.as_str(), "test-plugin");
        assert_eq!(source.root(), root);
    }

    #[test]
    fn read_wasm_binary() {
        let wasm_bytes = b"\x00asm fake component";
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", wasm_bytes)],
        );

        let source = DirectorySource::open(&root).expect("should open");
        let bytes = source.read_wasm().expect("should read wasm");
        assert_eq!(bytes, wasm_bytes);
    }

    #[test]
    fn read_nested_file() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[
                ("plugin.wasm", b"wasm"),
                ("frontend/launcher.js", b"export function View() {}"),
            ],
        );

        let source = DirectorySource::open(&root).expect("should open");
        let js = source.read_file("frontend/launcher.js").expect("should read");
        assert_eq!(js, b"export function View() {}");
    }

    #[test]
    fn reject_path_traversal() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm")],
        );

        let source = DirectorySource::open(&root).expect("should open");
        let result = source.read_file("../../../etc/passwd");
        assert!(result.is_err(), "should reject path traversal");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("escapes"),
            "error should mention escaping"
        );
    }

    #[test]
    fn reject_missing_manifest() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let result = DirectorySource::open(dir.path());
        assert!(result.is_err(), "should fail without manifest");
    }

    #[test]
    fn reject_invalid_manifest() {
        let (_dir, root) = make_plugin_dir(
            "not valid toml [[[",
            &[],
        );

        let result = DirectorySource::open(&root);
        assert!(result.is_err(), "should fail with invalid toml");
    }

    #[test]
    fn reject_nonexistent_file() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm")],
        );

        let source = DirectorySource::open(&root).expect("should open");
        let result = source.read_file("does-not-exist.txt");
        assert!(result.is_err(), "should fail for missing file");
    }

    #[test]
    fn read_wasm_fails_if_wasm_file_missing() {
        // Manifest references plugin.wasm but we don't create it.
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[]);

        let source = DirectorySource::open(&root).expect("should open");
        let result = source.read_wasm();
        assert!(result.is_err(), "should fail when wasm file is missing");
    }

    #[test]
    fn read_file_with_dot_segments() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[
                ("plugin.wasm", b"wasm"),
                ("frontend/launcher.js", b"js content"),
            ],
        );

        let source = DirectorySource::open(&root).expect("should open");

        // "frontend/../frontend/launcher.js" should resolve
        // to "frontend/launcher.js" and succeed.
        let result = source.read_file("frontend/../frontend/launcher.js");
        assert!(result.is_ok(), "dot segments within bounds should work");
        assert_eq!(result.unwrap(), b"js content");
    }

    #[test]
    fn reject_traversal_via_dot_segments_to_sibling() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm")],
        );

        let source = DirectorySource::open(&root).expect("should open");

        // Try to escape via "frontend/../../other-dir/secret"
        let result = source.read_file("frontend/../../other-dir/secret");
        assert!(result.is_err(), "should reject escape via dot segments");
    }

    #[test]
    fn read_deeply_nested_file() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[
                ("plugin.wasm", b"wasm"),
                ("a/b/c/d/deep.txt", b"deep content"),
            ],
        );

        let source = DirectorySource::open(&root).expect("should open");
        let content = source.read_file("a/b/c/d/deep.txt").expect("should read");
        assert_eq!(content, b"deep content");
    }

    #[test]
    fn read_binary_file_preserves_bytes() {
        // Ensure we handle non-UTF-8 binary data correctly.
        let binary: Vec<u8> = (0..=255).collect();
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[
                ("plugin.wasm", b"wasm"),
                ("data.bin", &binary),
            ],
        );

        let source = DirectorySource::open(&root).expect("should open");
        let content = source.read_file("data.bin").expect("should read");
        assert_eq!(content, binary);
    }

    #[test]
    fn manifest_accessible_after_open() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm")],
        );

        let source = DirectorySource::open(&root).expect("should open");

        // Verify the full manifest is accessible through the trait.
        let manifest = source.manifest();
        assert_eq!(manifest.plugin.id.as_str(), "test-plugin");
        assert_eq!(manifest.plugin.name, "Test Plugin");
        assert_eq!(manifest.plugin.description, "A test plugin");
        assert_eq!(manifest.plugin.version, "0.1.0");
        assert_eq!(manifest.plugin.wasm, "plugin.wasm");
    }

    #[test]
    fn open_nonexistent_directory() {
        let result = DirectorySource::open("/nonexistent/path/to/plugin");
        assert!(result.is_err(), "should fail for nonexistent directory");
    }

    // =====================================================
    // normalize_path unit tests
    // =====================================================

    #[test]
    fn normalize_resolves_parent_dir() {
        let input = Path::new("/a/b/../c");
        let normalized = normalize_path(input);
        assert_eq!(normalized, PathBuf::from("/a/c"));
    }

    #[test]
    fn normalize_resolves_current_dir() {
        let input = Path::new("/a/./b/./c");
        let normalized = normalize_path(input);
        assert_eq!(normalized, PathBuf::from("/a/b/c"));
    }

    #[test]
    fn normalize_handles_excessive_parent_dirs() {
        // Going above root should just stop at root.
        let input = Path::new("/a/../../../b");
        let normalized = normalize_path(input);
        assert_eq!(normalized, PathBuf::from("/b"));
    }

    #[test]
    fn normalize_preserves_absolute_path() {
        let input = Path::new("/a/b/c");
        let normalized = normalize_path(input);
        assert_eq!(normalized, PathBuf::from("/a/b/c"));
    }
}
