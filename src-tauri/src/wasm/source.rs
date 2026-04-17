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
// which source loaded the plugin — the source kind
// (`PluginSourceKind`) is tracked separately by the host so
// UI surfaces (badges, uninstall availability) can reason
// about where a plugin came from.
// =========================================================

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::Context as _;

use super::manifest::Manifest;

// =========================================================
// PluginSourceKind
// =========================================================

/// Tags each loaded plugin with the root it was discovered
/// from. Orthogonal to [`PluginSource`]: the trait abstracts
/// *how* we read the plugin's files (directory vs. archive),
/// this enum records *where on the host* it came from.
///
/// The UI uses this to:
///
/// - Show a source badge next to each plugin.
/// - Gate the uninstall action to `User` only.
/// - Reject install-time ID collisions against `Builtin`,
///   `System`, and `Dev` plugins.
///
/// Serialized as lowercase strings (`builtin`, `system`,
/// `user`, `dev`) across the Tauri IPC boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginSourceKind {
    /// Native Rust plugin compiled directly into the host
    /// binary (e.g. `clipboard`, `bangs`). Always present;
    /// not uninstallable.
    Builtin,
    /// WASM plugin shipped inside the application bundle
    /// under `<resource_dir>/plugins/`. Upgraded with the
    /// app; not uninstallable at runtime.
    System,
    /// WASM plugin installed by the user under
    /// `<app_data_dir>/plugins/`. Uninstallable from the
    /// Plugins settings panel.
    User,
    /// WASM plugin loaded from the repo-relative development
    /// path (`CARGO_MANIFEST_DIR/../plugins`) in debug
    /// builds. Skipped entirely in release builds.
    Dev,
}

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

        let toml_source = std::fs::read_to_string(&manifest_path)
            .with_context(|| format!("reading manifest at {}", manifest_path.display()))?;

        let manifest = Manifest::parse(&toml_source)
            .with_context(|| format!("parsing manifest at {}", manifest_path.display()))?;

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

        std::fs::read(&canonical).with_context(|| format!("reading plugin file `{path}`"))
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
// ArchiveSource
// =========================================================

/// Loads a plugin from a `.torchsnap` zip archive.
///
/// The archive is held open for the lifetime of the source so
/// that files (WASM binary, frontend assets, images) can be
/// read on demand without eagerly loading everything into
/// memory.
///
/// The manifest is parsed during `open()` and cached — reading
/// it does not require locking the archive.
pub struct ArchiveSource {
    /// The zip archive handle, behind a Mutex because
    /// `ZipArchive::by_name` requires `&mut self` (it seeks
    /// the underlying file).
    archive: Mutex<zip::ZipArchive<std::fs::File>>,
    manifest: Manifest,
}

impl ArchiveSource {
    /// Open a `.torchsnap` zip archive and parse its manifest.
    pub fn open(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let file = std::fs::File::open(path)
            .with_context(|| format!("opening archive at {}", path.display()))?;

        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("reading zip archive at {}", path.display()))?;

        // Read and parse manifest.toml from the archive.
        let manifest = {
            let mut entry = archive.by_name("manifest.toml").with_context(|| {
                format!(
                    "archive at {} does not contain manifest.toml",
                    path.display()
                )
            })?;

            let mut toml_source = String::new();
            entry
                .read_to_string(&mut toml_source)
                .context("reading manifest.toml from archive")?;

            Manifest::parse(&toml_source).with_context(|| {
                format!("parsing manifest.toml in archive at {}", path.display())
            })?
        };

        Ok(Self {
            archive: Mutex::new(archive),
            manifest,
        })
    }
}

impl PluginSource for ArchiveSource {
    fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    fn read_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        // Reject paths that attempt to escape the archive root.
        // Zip entry names are relative strings — we can't use
        // filesystem canonicalization, so we track directory
        // depth during normalization. If it ever goes negative,
        // the path escapes the root.
        anyhow::ensure!(
            !path.starts_with('/'),
            "plugin file path `{path}` must be relative"
        );

        let normalized = normalize_path(Path::new(path));
        let normalized_str = normalized.to_string_lossy();

        // Walk components and track depth. A `..` that would
        // go above the root (depth < 0) is a traversal attempt.
        let mut depth: i32 = 0;
        for component in Path::new(path).components() {
            match component {
                std::path::Component::ParentDir => depth -= 1,
                std::path::Component::Normal(_) => depth += 1,
                _ => {}
            }
            anyhow::ensure!(
                depth >= 0,
                "plugin file path `{path}` escapes the archive root"
            );
        }

        let mut archive = self.archive.lock().expect("archive mutex not poisoned");

        let mut entry = archive
            .by_name(&normalized_str)
            .with_context(|| format!("reading plugin file `{path}` from archive"))?;

        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut buf)
            .with_context(|| format!("decompressing plugin file `{path}`"))?;

        Ok(buf)
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================
    // PluginSourceKind serialization
    // =========================================================

    /// Every variant must round-trip through JSON unchanged.
    /// The IPC boundary with the frontend uses JSON, so any
    /// serializer/deserializer asymmetry would surface as a
    /// silently-lost source kind on the UI.
    #[test]
    fn plugin_source_kind_round_trips_through_json() {
        for kind in [
            PluginSourceKind::Builtin,
            PluginSourceKind::System,
            PluginSourceKind::User,
            PluginSourceKind::Dev,
        ] {
            let encoded = serde_json::to_string(&kind).expect("serialize");
            let decoded: PluginSourceKind = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, kind, "round-trip lost information for {kind:?}");
        }
    }

    /// The wire format must stay lowercase. The TypeScript
    /// `PluginSourceKind` string literal type depends on these
    /// exact values — if Rust ever produces `"Builtin"` or
    /// `"BUILTIN"` instead of `"builtin"`, the frontend will
    /// silently fail to match.
    #[test]
    fn plugin_source_kind_uses_lowercase_wire_names() {
        let cases = [
            (PluginSourceKind::Builtin, "\"builtin\""),
            (PluginSourceKind::System, "\"system\""),
            (PluginSourceKind::User, "\"user\""),
            (PluginSourceKind::Dev, "\"dev\""),
        ];
        for (kind, expected) in cases {
            let encoded = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(encoded, expected);
        }
    }

    /// Deserialization of any other casing must fail — we
    /// rely on this strictness so typos in a consumer are
    /// caught rather than silently producing a wrong kind.
    #[test]
    fn plugin_source_kind_rejects_non_lowercase_input() {
        for bad in ["\"Builtin\"", "\"SYSTEM\"", "\"User \"", "\"\"", "null"] {
            let decoded: Result<PluginSourceKind, _> = serde_json::from_str(bad);
            assert!(
                decoded.is_err(),
                "expected rejection for input {bad}, got {decoded:?}"
            );
        }
    }

    /// Helper: create a temporary plugin directory with a
    /// manifest and optional extra files.
    fn make_plugin_dir(
        manifest_toml: &str,
        files: &[(&str, &[u8])],
    ) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let root = dir.path().to_path_buf();

        std::fs::write(root.join("manifest.toml"), manifest_toml).expect("write manifest");

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
        let (_dir, root) =
            make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"fake wasm bytes")]);

        let source = DirectorySource::open(&root).expect("should open");
        assert_eq!(source.manifest().plugin.id.as_str(), "test-plugin");
        assert_eq!(source.root(), root);
    }

    #[test]
    fn read_wasm_binary() {
        let wasm_bytes = b"\x00asm fake component";
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", wasm_bytes)]);

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
        let js = source
            .read_file("frontend/launcher.js")
            .expect("should read");
        assert_eq!(js, b"export function View() {}");
    }

    #[test]
    fn reject_path_traversal() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

        let source = DirectorySource::open(&root).expect("should open");
        let result = source.read_file("../../../etc/passwd");
        assert!(result.is_err(), "should reject path traversal");
        assert!(
            result.unwrap_err().to_string().contains("escapes"),
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
        let (_dir, root) = make_plugin_dir("not valid toml [[[", &[]);

        let result = DirectorySource::open(&root);
        assert!(result.is_err(), "should fail with invalid toml");
    }

    #[test]
    fn reject_nonexistent_file() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

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
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

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
            &[("plugin.wasm", b"wasm"), ("data.bin", &binary)],
        );

        let source = DirectorySource::open(&root).expect("should open");
        let content = source.read_file("data.bin").expect("should read");
        assert_eq!(content, binary);
    }

    #[test]
    fn manifest_accessible_after_open() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

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

    // =====================================================
    // ArchiveSource tests
    //
    // Each test builds an in-memory zip archive via
    // ZipWriter, writes it to a temp file, then opens it
    // with ArchiveSource.
    // =====================================================

    /// Helper: build a `.torchsnap` zip file in a temp directory.
    /// Returns the temp dir (for lifetime) and the archive path.
    fn make_archive(manifest_toml: &str, files: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf) {
        use std::io::{Cursor, Write as _};
        use zip::ZipWriter;
        use zip::write::SimpleFileOptions;

        let mut buf = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buf);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            writer
                .start_file("manifest.toml", options)
                .expect("start manifest entry");
            writer
                .write_all(manifest_toml.as_bytes())
                .expect("write manifest");

            for (path, contents) in files {
                writer.start_file(*path, options).expect("start file entry");
                writer.write_all(contents).expect("write file");
            }

            writer.finish().expect("finalize zip");
        }

        let dir = tempfile::tempdir().expect("create temp dir");
        let archive_path = dir.path().join("plugin.torchsnap");
        std::fs::write(&archive_path, buf.into_inner()).expect("write archive");

        (dir, archive_path)
    }

    /// Helper: build a zip without a manifest.toml.
    fn make_archive_without_manifest(files: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf) {
        use std::io::{Cursor, Write as _};
        use zip::ZipWriter;
        use zip::write::SimpleFileOptions;

        let mut buf = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut buf);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

            for (path, contents) in files {
                writer.start_file(*path, options).expect("start file entry");
                writer.write_all(contents).expect("write file");
            }

            writer.finish().expect("finalize zip");
        }

        let dir = tempfile::tempdir().expect("create temp dir");
        let archive_path = dir.path().join("plugin.torchsnap");
        std::fs::write(&archive_path, buf.into_inner()).expect("write archive");

        (dir, archive_path)
    }

    #[test]
    fn archive_open_valid() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"fake wasm")]);

        let source = ArchiveSource::open(&path).expect("should open");
        assert_eq!(source.manifest().plugin.id.as_str(), "test-plugin");
    }

    #[test]
    fn archive_read_wasm() {
        let wasm_bytes = b"\x00asm fake component";
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", wasm_bytes)]);

        let source = ArchiveSource::open(&path).expect("should open");
        let bytes = source.read_wasm().expect("should read wasm");
        assert_eq!(bytes, wasm_bytes);
    }

    #[test]
    fn archive_read_nested_file() {
        let (_dir, path) = make_archive(
            MINIMAL_MANIFEST,
            &[
                ("plugin.wasm", b"wasm"),
                ("frontend/launcher.js", b"export function View() {}"),
            ],
        );

        let source = ArchiveSource::open(&path).expect("should open");
        let js = source
            .read_file("frontend/launcher.js")
            .expect("should read");
        assert_eq!(js, b"export function View() {}");
    }

    #[test]
    fn archive_read_binary_preserves_bytes() {
        let binary: Vec<u8> = (0..=255).collect();
        let (_dir, path) = make_archive(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm"), ("data.bin", &binary)],
        );

        let source = ArchiveSource::open(&path).expect("should open");
        let content = source.read_file("data.bin").expect("should read");
        assert_eq!(content, binary);
    }

    #[test]
    fn archive_manifest_accessible() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

        let source = ArchiveSource::open(&path).expect("should open");
        let manifest = source.manifest();
        assert_eq!(manifest.plugin.id.as_str(), "test-plugin");
        assert_eq!(manifest.plugin.name, "Test Plugin");
        assert_eq!(manifest.plugin.description, "A test plugin");
        assert_eq!(manifest.plugin.version, "0.1.0");
        assert_eq!(manifest.plugin.wasm, "plugin.wasm");
    }

    #[test]
    fn archive_reject_missing_manifest() {
        let (_dir, path) = make_archive_without_manifest(&[("plugin.wasm", b"wasm")]);

        let result = ArchiveSource::open(&path);
        assert!(result.is_err(), "should fail without manifest.toml");
    }

    #[test]
    fn archive_reject_invalid_manifest() {
        let (_dir, path) = make_archive("not valid toml [[[", &[]);

        let result = ArchiveSource::open(&path);
        assert!(result.is_err(), "should fail with invalid toml");
    }

    #[test]
    fn archive_reject_nonexistent_file() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

        let source = ArchiveSource::open(&path).expect("should open");
        let result = source.read_file("does-not-exist.txt");
        assert!(result.is_err(), "should fail for missing file");
    }

    #[test]
    fn archive_reject_path_traversal() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

        let source = ArchiveSource::open(&path).expect("should open");
        let result = source.read_file("../../../etc/passwd");
        assert!(result.is_err(), "should reject path traversal");
        assert!(
            result.unwrap_err().to_string().contains("escapes"),
            "error should mention escaping"
        );
    }

    #[test]
    fn archive_reject_dot_segment_escape() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

        let source = ArchiveSource::open(&path).expect("should open");
        let result = source.read_file("frontend/../../secret");
        assert!(result.is_err(), "should reject escape via dot segments");
    }

    #[test]
    fn archive_reject_absolute_path() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);

        let source = ArchiveSource::open(&path).expect("should open");
        let result = source.read_file("/etc/passwd");
        assert!(result.is_err(), "should reject absolute path");
        assert!(
            result.unwrap_err().to_string().contains("relative"),
            "error should mention relative"
        );
    }

    #[test]
    fn archive_read_wasm_missing() {
        // Manifest references plugin.wasm but archive doesn't contain it.
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[]);

        let source = ArchiveSource::open(&path).expect("should open");
        let result = source.read_wasm();
        assert!(result.is_err(), "should fail when wasm file is missing");
    }

    #[test]
    fn archive_not_a_zip() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("not-a-zip.torchsnap");
        std::fs::write(&path, b"this is not a zip file").expect("write");

        let result = ArchiveSource::open(&path);
        assert!(result.is_err(), "should fail for non-zip file");
    }

    #[test]
    fn archive_empty_zip() {
        use std::io::Cursor;
        use zip::ZipWriter;

        // Create a valid but empty zip (no entries at all).
        let mut buf = Cursor::new(Vec::new());
        {
            let writer = ZipWriter::new(&mut buf);
            writer.finish().expect("finalize empty zip");
        }

        let dir = tempfile::tempdir().expect("create temp dir");
        let path = dir.path().join("empty.torchsnap");
        std::fs::write(&path, buf.into_inner()).expect("write");

        let result = ArchiveSource::open(&path);
        assert!(result.is_err(), "should fail for empty zip (no manifest)");
    }

    #[test]
    fn archive_nonexistent_path() {
        let result = ArchiveSource::open("/nonexistent/path/to/plugin.torchsnap");
        assert!(result.is_err(), "should fail for nonexistent archive");
    }
}
