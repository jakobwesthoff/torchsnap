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

    /// Check whether a file exists at `path` without
    /// reading its bytes.
    ///
    /// Used by the `assets::exists` host import so plugins
    /// can probe optional assets cheaply. `path` is
    /// validated with the same `validate_plugin_path` guard
    /// as `read_file` — traversal, absolute paths, etc. are
    /// rejected as errors (not `Ok(false)`) so callers can
    /// tell "invalid path" apart from "valid path, file
    /// absent".
    ///
    /// No default impl: the directory and archive backends
    /// need different code to answer without paying the
    /// cost of a full read.
    fn file_exists(&self, path: &str) -> anyhow::Result<bool>;

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
        // First, the lexical guard — catches absolute paths,
        // traversal, Windows-style roots, backslashes, NULs,
        // and anything else before we touch the filesystem.
        validate_plugin_path(path)?;

        let full_path = self.root.join(path);

        // Second, a canonicalize-based check that follows
        // symlinks. A plugin directory containing a symlink
        // into the host's filesystem would pass the lexical
        // guard but be caught here.
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

    fn file_exists(&self, path: &str) -> anyhow::Result<bool> {
        // Same lexical guard as `read_file`: traversal,
        // absolute paths, backslashes, etc. surface as
        // errors (Err), not as "valid path, missing file"
        // (Ok(false)).
        validate_plugin_path(path)?;

        let full_path = self.root.join(path);

        let canonical_root = self
            .root
            .canonicalize()
            .context("resolving plugin root directory")?;

        // If the file doesn't exist, `validate_plugin_path`
        // above has already established that the lexical
        // path sits inside the plugin root. There is no
        // symlink target to inspect in the "doesn't exist"
        // branch, so we don't need a canonicalization-based
        // root check here (doing one is fragile: on macOS
        // the plugin root's canonical form is
        // `/private/var/...` while the lexical join yields
        // `/var/...`, producing false-negative "escapes"
        // errors). Return `Ok(false)` directly.
        let canonical = match full_path.canonicalize() {
            Ok(p) => p,
            Err(_) => return Ok(false),
        };

        // Canonicalize succeeded — the entry exists. Verify
        // it didn't resolve outside the root via a symlink
        // that `validate_plugin_path` couldn't see.
        anyhow::ensure!(
            canonical.starts_with(&canonical_root),
            "plugin file path `{path}` escapes the plugin directory"
        );

        // Require a regular file so `file_exists("some-subdir")`
        // reports `false` for directories rather than
        // misleading callers.
        Ok(canonical.is_file())
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
// Plugin Path Guard
//
// Single lexical validator for every user-supplied path
// inside a plugin — manifest-referenced files (wasm, icon,
// migrations, frontend bundles/CSS) and every argument to
// `read_file()`.
//
// A manifest that says `launcher-bundle = "../../.ssh/id_rsa"`
// would otherwise get the host to read an arbitrary file and
// hand the bytes back as a "frontend bundle". The host would
// gladly serve that to the webview via the plugin protocol.
// The WIT sandbox does not cover this path because the read
// happens host-side before anything reaches the guest.
//
// Applied at two layers:
//
// 1. **Manifest parse** — every path field in `manifest.toml`
//    is validated before the plugin is considered loadable.
//    This is the primary gate.
// 2. **Read boundary** — both `DirectorySource::read_file` and
//    `ArchiveSource::read_file` re-validate their `path`
//    argument. Defense-in-depth against a host bug that ever
//    forwards an unvalidated path to the source layer.
//
// Rules the guard enforces:
//
// - Non-empty.
// - No NUL bytes (defensive against embedded-null path
//   truncation surprises on some platforms).
// - No backslashes — paths inside a plugin are always
//   forward-slash, regardless of host OS. Archives use the
//   zip spec (forward-slash), directory manifests are
//   cross-platform by policy.
// - Not an absolute POSIX path (leading `/`).
// - Not a Windows absolute path (`C:\…`, `\\…`). This guard
//   fires even on macOS/Linux so a Windows-targeted malicious
//   plugin still gets rejected before reaching platform-
//   specific code.
// - After lexically resolving `..` and `.` segments, the
//   running depth never goes below zero (i.e. the path never
//   escapes the plugin root).
//
// Returns the lexically normalized path for callers that want
// to use it as a filesystem-side key. The unnormalized input
// is still carried in error messages to help plugin authors
// debug rejections.
// =========================================================

/// Validate that `path` is a safe, plugin-relative path.
/// Returns the lexically normalized form on success. See the
/// module-level "Plugin Path Guard" comment for the full list
/// of rules.
pub(crate) fn validate_plugin_path(path: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(!path.is_empty(), "plugin file path must not be empty");
    anyhow::ensure!(
        !path.contains('\0'),
        "plugin file path `{path}` must not contain NUL bytes"
    );
    anyhow::ensure!(
        !path.contains('\\'),
        "plugin file path `{path}` must use forward slashes (`/`) — backslashes are rejected regardless of host OS"
    );
    anyhow::ensure!(
        !path.starts_with('/'),
        "plugin file path `{path}` must be relative (no leading `/`)"
    );

    // Windows drive-letter detection (`C:`, `z:`, …). Rejected
    // on all platforms so a malicious plugin shipped from a
    // Windows author still fails on macOS/Linux.
    let mut bytes = path.bytes();
    if let (Some(first), Some(second)) = (bytes.next(), bytes.next())
        && first.is_ascii_alphabetic()
        && second == b':'
    {
        anyhow::bail!(
            "plugin file path `{path}` looks like a Windows absolute path — paths must be plugin-relative"
        );
    }

    // Walk the components in order, tracking the running
    // depth. A path that drops below zero at any point is
    // escaping — catches `../foo` and also the subtler
    // `a/../../bar` where the final normalized form may
    // happen to land inside the root but the walk crossed
    // the boundary.
    let mut depth: i32 = 0;
    for component in Path::new(path).components() {
        match component {
            std::path::Component::ParentDir => {
                depth -= 1;
                anyhow::ensure!(
                    depth >= 0,
                    "plugin file path `{path}` escapes the plugin root"
                );
            }
            std::path::Component::Normal(_) => depth += 1,
            std::path::Component::CurDir => {}
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                // Prefix covers Windows `\\?\` and drive
                // prefixes in case an exotic input slipped
                // past the earlier checks.
                anyhow::bail!(
                    "plugin file path `{path}` must be relative (no root or drive prefix)"
                );
            }
        }
    }

    Ok(normalize_path(Path::new(path)))
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
        // Validate and normalize before touching the archive.
        // The guard rejects absolute / Windows / backslash /
        // NUL / traversal paths in one place; the normalized
        // output is what zip `by_name` lookups should use.
        let normalized = validate_plugin_path(path)?;
        let normalized_str = normalized.to_string_lossy();

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

    fn file_exists(&self, path: &str) -> anyhow::Result<bool> {
        // Same validation contract as `read_file`: invalid
        // paths are errors, not `Ok(false)`.
        let normalized = validate_plugin_path(path)?;
        let normalized_str = normalized.to_string_lossy();

        let mut archive = self.archive.lock().expect("archive mutex not poisoned");

        // Distinguish "valid lookup, no such entry" (`Ok(false)`)
        // from genuine archive errors (propagated). The zip
        // crate's `by_name` surfaces a missing entry as
        // `ZipError::FileNotFound`; anything else (IO,
        // corruption) is a real problem for the caller.
        match archive.by_name(&normalized_str) {
            Ok(_) => Ok(true),
            Err(zip::result::ZipError::FileNotFound) => Ok(false),
            Err(e) => Err(anyhow::anyhow!("archive lookup for `{path}` failed: {e}")),
        }
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

    // =========================================================
    // validate_plugin_path: the accept-cases
    //
    // Every path a legitimate plugin author might write must
    // pass. Failing any of these would break the real-world
    // plugins (calculator, clipboard, hello-world, template)
    // when they reference their own files.
    // =========================================================

    #[test]
    fn path_guard_accepts_bare_filename() {
        assert!(validate_plugin_path("manifest.toml").is_ok());
        assert!(validate_plugin_path("icon.webp").is_ok());
    }

    #[test]
    fn path_guard_accepts_nested_file() {
        assert!(validate_plugin_path("frontend/dist/launcher.js").is_ok());
        assert!(validate_plugin_path("migrations/001_init.sql").is_ok());
    }

    #[test]
    fn path_guard_accepts_deeply_nested_path() {
        assert!(validate_plugin_path("a/b/c/d/e/deeply_nested.bin").is_ok());
    }

    #[test]
    fn path_guard_accepts_dots_within_components() {
        // A filename that contains dots but is not a `..`
        // segment must pass — `.env.local`, `foo.bar.baz`,
        // etc. are legitimate filenames.
        assert!(validate_plugin_path("foo.bar.baz.wasm").is_ok());
        assert!(validate_plugin_path("frontend/.prettierrc.json").is_ok());
    }

    #[test]
    fn path_guard_accepts_roundtrip_into_self() {
        // `a/./b` normalizes to `a/b` — curdir segments are
        // valid even though they're unusual.
        assert!(validate_plugin_path("frontend/./launcher.js").is_ok());
    }

    #[test]
    fn path_guard_returns_normalized_output() {
        // The normalized form strips `./` and dedupes
        // separators. Callers use this for zip `by_name`
        // lookups so it must match what a well-behaved
        // manifest author would have written.
        let normalized = validate_plugin_path("frontend/./dist/launcher.js").unwrap();
        assert_eq!(normalized, Path::new("frontend/dist/launcher.js"));
    }

    // =========================================================
    // validate_plugin_path: the reject-cases
    //
    // Every form of attacker-controlled path that could
    // escape the plugin root must be rejected. Failures here
    // are the exact scenarios documented in the Plugin Path
    // Guard comment above.
    // =========================================================

    #[test]
    fn path_guard_rejects_empty_string() {
        assert!(validate_plugin_path("").is_err());
    }

    #[test]
    fn path_guard_rejects_nul_byte() {
        assert!(validate_plugin_path("foo\0bar").is_err());
        assert!(validate_plugin_path("\0").is_err());
    }

    #[test]
    fn path_guard_rejects_backslash_separator() {
        // Even on Windows we reject backslashes: plugin
        // paths are forward-slash by policy, matching the
        // zip spec. This keeps cross-platform behaviour
        // uniform.
        assert!(validate_plugin_path("frontend\\launcher.js").is_err());
        assert!(validate_plugin_path("..\\escape").is_err());
    }

    #[test]
    fn path_guard_rejects_leading_slash() {
        assert!(validate_plugin_path("/etc/passwd").is_err());
        assert!(validate_plugin_path("/").is_err());
    }

    #[test]
    fn path_guard_rejects_windows_drive_letter() {
        assert!(validate_plugin_path("C:/Windows/System32/cmd.exe").is_err());
        assert!(validate_plugin_path("z:foo").is_err());
    }

    #[test]
    fn path_guard_rejects_parent_at_start() {
        assert!(validate_plugin_path("../secret").is_err());
        assert!(validate_plugin_path("..").is_err());
    }

    #[test]
    fn path_guard_rejects_parent_in_middle() {
        // The walking check catches paths where the running
        // depth dips below zero, even if the end result
        // happens to land back inside the root.
        assert!(validate_plugin_path("a/../../outside").is_err());
    }

    #[test]
    fn path_guard_rejects_nested_parent_traversal() {
        assert!(validate_plugin_path("../../../../etc/shadow").is_err());
        assert!(validate_plugin_path("frontend/../../escape").is_err());
    }

    /// Regression guard for the "crosses zero then returns"
    /// case: `a/../../b/c/d` normalizes to `b/c/d` which
    /// lands inside the root, but the walk visits depth -1
    /// in the middle — an attacker could otherwise abuse
    /// this to probe the filesystem structure outside the
    /// plugin.
    #[test]
    fn path_guard_rejects_depth_dip_even_if_final_inside_root() {
        assert!(validate_plugin_path("a/../../b/c").is_err());
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

    // =====================================================
    // file_exists — DirectorySource
    //
    // Coverage target: the full happy / missing matrix plus
    // every rejection category `validate_plugin_path` can
    // surface. `file_exists` must share the same lexical
    // guard as `read_file`; the tests assert behavioral
    // parity for the guard paths so a future divergence
    // (e.g. someone forgetting to call the guard) breaks
    // here first.
    // =====================================================

    #[test]
    fn file_exists_returns_true_for_existing_directory_file() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm"), ("data/bangs.json", b"[]")],
        );
        let source = DirectorySource::open(&root).expect("open");
        assert!(
            source
                .file_exists("data/bangs.json")
                .expect("probe succeeds")
        );
    }

    #[test]
    fn file_exists_returns_false_for_missing_directory_file() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = DirectorySource::open(&root).expect("open");
        assert!(
            !source
                .file_exists("not-here.json")
                .expect("probe succeeds even for missing")
        );
    }

    #[test]
    fn file_exists_returns_false_for_directory_entry() {
        // `file_exists` reports true only for regular files.
        // A subdirectory returns `Ok(false)` so callers
        // don't treat it as a readable asset.
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm"), ("assets/thing.txt", b"hi")],
        );
        let source = DirectorySource::open(&root).expect("open");
        assert!(
            !source
                .file_exists("assets")
                .expect("probe succeeds for dir"),
            "directory entries are not files"
        );
    }

    #[test]
    fn file_exists_rejects_traversal_path() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = DirectorySource::open(&root).expect("open");
        let err = source
            .file_exists("../../../etc/passwd")
            .expect_err("traversal must error");
        assert!(
            err.to_string().contains("escapes"),
            "error should match read_file's traversal message (got: {err})"
        );
    }

    #[test]
    fn file_exists_rejects_absolute_path() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = DirectorySource::open(&root).expect("open");
        let err = source
            .file_exists("/etc/passwd")
            .expect_err("absolute must error");
        assert!(
            err.to_string().contains("relative"),
            "error should mention relative (got: {err})"
        );
    }

    #[test]
    fn file_exists_rejects_backslash_path() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = DirectorySource::open(&root).expect("open");
        let err = source
            .file_exists("frontend\\launcher.js")
            .expect_err("backslash must error");
        assert!(
            err.to_string().contains("backslash"),
            "error should mention backslash (got: {err})"
        );
    }

    #[test]
    fn file_exists_rejects_empty_path() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = DirectorySource::open(&root).expect("open");
        let err = source.file_exists("").expect_err("empty must error");
        assert!(
            err.to_string().contains("empty"),
            "error should mention empty (got: {err})"
        );
    }

    #[test]
    fn file_exists_rejects_nul_byte_path() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = DirectorySource::open(&root).expect("open");
        let err = source
            .file_exists("data\0hidden")
            .expect_err("NUL must error");
        assert!(
            err.to_string().contains("NUL"),
            "error should mention NUL (got: {err})"
        );
    }

    #[test]
    fn file_exists_rejects_windows_drive_letter() {
        let (_dir, root) = make_plugin_dir(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = DirectorySource::open(&root).expect("open");
        let err = source
            .file_exists("C:/Windows/System32/config/SAM")
            .expect_err("drive letter must error");
        let msg = err.to_string();
        // The guard rejects via either the Windows-drive
        // branch or the backslash-rule depending on the
        // normalizer; both paths count as a rejection.
        assert!(
            msg.contains("Windows") || msg.contains("relative") || msg.contains("backslash"),
            "error should identify the path as absolute/windows (got: {msg})"
        );
    }

    #[test]
    fn file_exists_handles_dot_segments_within_bounds() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[
                ("plugin.wasm", b"wasm"),
                ("frontend/launcher.js", b"js content"),
            ],
        );
        let source = DirectorySource::open(&root).expect("open");
        assert!(
            source
                .file_exists("frontend/../frontend/launcher.js")
                .expect("dot segments in bounds are fine")
        );
    }

    #[test]
    fn file_exists_returns_false_after_file_removed() {
        let (dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm"), ("transient.txt", b"will go")],
        );
        let source = DirectorySource::open(&root).expect("open");
        assert!(source.file_exists("transient.txt").expect("present"));
        std::fs::remove_file(dir.path().join("transient.txt")).expect("rm");
        assert!(
            !source
                .file_exists("transient.txt")
                .expect("absent is not an error"),
            "file should be gone after removal"
        );
    }

    #[test]
    fn file_exists_returns_true_for_deeply_nested_file() {
        let (_dir, root) = make_plugin_dir(
            MINIMAL_MANIFEST,
            &[
                ("plugin.wasm", b"wasm"),
                ("a/b/c/d/deep.txt", b"deep content"),
            ],
        );
        let source = DirectorySource::open(&root).expect("open");
        assert!(
            source
                .file_exists("a/b/c/d/deep.txt")
                .expect("deep probe ok")
        );
    }

    // =====================================================
    // file_exists — ArchiveSource
    //
    // The zip crate surfaces a missing entry as
    // `ZipError::FileNotFound`. The `archive_file_exists_*`
    // tests lock in that branch so a future zip crate
    // upgrade that renames the variant (or the
    // implementation that stops matching on it) trips here
    // before plugins regress.
    // =====================================================

    #[test]
    fn archive_file_exists_returns_true_for_existing_entry() {
        let (_dir, path) = make_archive(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm"), ("data/bangs.json", b"[]")],
        );
        let source = ArchiveSource::open(&path).expect("open");
        assert!(source.file_exists("data/bangs.json").expect("probe ok"));
    }

    #[test]
    fn archive_file_exists_returns_false_for_missing_entry() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = ArchiveSource::open(&path).expect("open");
        assert!(
            !source
                .file_exists("not-in-archive.txt")
                .expect("missing entry is Ok(false), not Err")
        );
    }

    #[test]
    fn archive_file_exists_rejects_traversal_path() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = ArchiveSource::open(&path).expect("open");
        let err = source
            .file_exists("../../../etc/passwd")
            .expect_err("traversal must error");
        assert!(
            err.to_string().contains("escapes"),
            "error should mention escaping (got: {err})"
        );
    }

    #[test]
    fn archive_file_exists_rejects_absolute_path() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = ArchiveSource::open(&path).expect("open");
        let err = source
            .file_exists("/etc/passwd")
            .expect_err("absolute must error");
        assert!(
            err.to_string().contains("relative"),
            "error should mention relative (got: {err})"
        );
    }

    #[test]
    fn archive_file_exists_rejects_backslash_path() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = ArchiveSource::open(&path).expect("open");
        let err = source
            .file_exists("frontend\\launcher.js")
            .expect_err("backslash must error");
        assert!(
            err.to_string().contains("backslash"),
            "error should mention backslash (got: {err})"
        );
    }

    #[test]
    fn archive_file_exists_rejects_empty_path() {
        let (_dir, path) = make_archive(MINIMAL_MANIFEST, &[("plugin.wasm", b"wasm")]);
        let source = ArchiveSource::open(&path).expect("open");
        let err = source.file_exists("").expect_err("empty must error");
        assert!(
            err.to_string().contains("empty"),
            "error should mention empty (got: {err})"
        );
    }

    #[test]
    fn archive_file_exists_handles_nested_entry() {
        let (_dir, path) = make_archive(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm"), ("a/b/c/d/deep.txt", b"deep")],
        );
        let source = ArchiveSource::open(&path).expect("open");
        assert!(source.file_exists("a/b/c/d/deep.txt").expect("probe ok"));
        assert!(
            !source
                .file_exists("a/b/c/d/not-there.txt")
                .expect("missing nested is Ok(false)")
        );
    }

    #[test]
    fn archive_file_exists_survives_multiple_probes() {
        // Regression guard: probing the archive must not
        // consume or corrupt the shared mutex-protected
        // reader. Run several reads / probes in sequence
        // and verify both shapes of call continue to work.
        let (_dir, path) = make_archive(
            MINIMAL_MANIFEST,
            &[("plugin.wasm", b"wasm"), ("a.txt", b"a"), ("b.txt", b"b")],
        );
        let source = ArchiveSource::open(&path).expect("open");
        assert!(source.file_exists("a.txt").expect("a"));
        assert_eq!(source.read_file("a.txt").expect("read a"), b"a");
        assert!(source.file_exists("b.txt").expect("b"));
        assert!(!source.file_exists("c.txt").expect("c missing"));
        assert_eq!(source.read_file("b.txt").expect("read b"), b"b");
    }
}
