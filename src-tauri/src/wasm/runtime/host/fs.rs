// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// FS host import
//
// Read-only filesystem access for WASM gadgets. The gadget
// declares an allowlist of path patterns under
// `[permissions.fs]` in its manifest; the bridge expands
// `${...}` substitution variables, canonicalizes the static
// portion of each pattern, compiles them into a `GlobSet`,
// and stashes that on `FsState` at `enable()`.
//
// At runtime, `read-file` / `file-exists` / `metadata`
// canonicalize the request path the same way and match the
// real path against the GlobSet. A pattern allows a request
// only if both have been resolved through symlinks; this is
// what makes "follow symlinks" safe, since a symlink inside
// an allowed location pointing outside the allow set is
// rejected after canonicalization.
//
// `std::fs::read` / `std::fs::canonicalize` block the OS
// thread, so the host trait impl wraps the I/O in
// `tokio::task::block_in_place` to keep other guest tasks
// runnable while one gadget is reading.
// =========================================================

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

use crate::wasm::bindings;
use crate::paths::PathResolver;

use super::super::GadgetState;

/// FS state stashed on `GadgetState`. `allowlist` is `None`
/// when the gadget's manifest has no `[permissions.fs]`
/// section, which means every fs call returns
/// `permission-denied`.
#[derive(Default)]
pub(crate) struct FsState {
    pub(crate) allowlist: Option<Arc<FsAllowlist>>,
}

/// Compiled fs allowlist for a single gadget instance.
///
/// `globset` is the matcher built from each pattern;
/// `canonical_patterns` is the human-readable form retained
/// only for diagnostics (e.g. logging which patterns a denied
/// request was checked against).
#[derive(Debug)]
pub(crate) struct FsAllowlist {
    pub(crate) globset: GlobSet,
    #[allow(dead_code)]
    pub(crate) canonical_patterns: Vec<String>,
}

/// Internal error for fs operations before mapping to the WIT
/// `fs-error` variant. `WasmFsError::*` mirrors the WIT cases
/// 1:1; the conversion `impl From` at the bottom of this
/// module performs the variant translation.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WasmFsError {
    #[error("path not permitted: {0}")]
    PermissionDenied(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("not found")]
    NotFound,
    #[error("io: {0}")]
    Io(String),
}

impl From<WasmFsError> for bindings::torchsnap::gadget::fs::FsError {
    fn from(e: WasmFsError) -> Self {
        use bindings::torchsnap::gadget::fs::FsError;
        match e {
            WasmFsError::PermissionDenied(msg) => FsError::PermissionDenied(msg),
            WasmFsError::InvalidPath(msg) => FsError::InvalidPath(msg),
            WasmFsError::NotFound => FsError::NotFound,
            WasmFsError::Io(msg) => FsError::Io(msg),
        }
    }
}

/// Compile a manifest's `read = [...]` patterns into a
/// `FsAllowlist`. Called by the bridge at construction time
/// once a [`PathResolver`] is available for variable
/// substitution.
///
/// For each pattern:
/// 1. Substitute `${...}` tokens via the resolver.
/// 2. Canonicalize the static prefix (everything up to the
///    first glob metacharacter), walking up through
///    non-existent components until an existing ancestor is
///    found. This resolves OS-level symlinks like macOS's
///    `/var → /private/var` so runtime canonicalize results
///    match patterns the manifest spelled with `/var`.
/// 3. Re-prepend the canonicalized prefix to the glob suffix.
/// 4. Compile with `literal_separator(true)` so `*` does not
///    cross `/`.
///
/// Patterns with traversal segments (`..`, `.`) or
/// unsupported glob metacharacters are rejected by the
/// manifest validator before this point — see
/// `validate_fs_pattern` in `manifest.rs`.
pub(crate) fn compile_fs_patterns(
    patterns: &[String],
    resolver: &impl PathResolver,
) -> anyhow::Result<FsAllowlist> {
    let mut builder = GlobSetBuilder::new();
    let mut canonical_patterns = Vec::with_capacity(patterns.len());

    for pattern in patterns {
        let substituted = resolver.substitute_variables(pattern).map_err(|e| {
            anyhow::anyhow!("`[permissions.fs]` pattern `{pattern}` substitution failed: {e}")
        })?;

        // Reject unsupported glob shapes against the
        // post-substitution form so the check sees the actual
        // characters the GlobSet would compile, with no
        // `${...}` tokens to work around.
        if let Some(ch) = substituted
            .chars()
            .find(|c| matches!(c, '?' | '[' | ']' | '{' | '}'))
        {
            anyhow::bail!(
                "`[permissions.fs]` pattern `{pattern}` uses unsupported glob \
                 metacharacter `{ch}`; only `*` (single segment) and `**` \
                 (multi-segment) are accepted"
            );
        }

        let canonical = canonicalize_pattern(&substituted);
        let glob = GlobBuilder::new(&canonical)
            .literal_separator(true)
            .build()
            .map_err(|e| {
                anyhow::anyhow!(
                    "`[permissions.fs]` pattern `{pattern}` (resolved to `{canonical}`) \
                     is not a valid glob: {e}"
                )
            })?;
        builder.add(glob);
        canonical_patterns.push(canonical);
    }

    let globset = builder
        .build()
        .map_err(|e| anyhow::anyhow!("`[permissions.fs]` glob set assembly failed: {e}"))?;

    Ok(FsAllowlist {
        globset,
        canonical_patterns,
    })
}

/// Canonicalize the static prefix of `pattern` so the result
/// reflects real OS paths (resolving symlinks like macOS's
/// `/var → /private/var`). Glob metacharacters in the suffix
/// are preserved verbatim.
///
/// If the pattern's deepest existing ancestor cannot be
/// canonicalized (filesystem error, weird permissions), the
/// original pattern is returned unchanged — better to compile
/// a possibly-non-matching glob than to fail manifest load on
/// a transient filesystem state.
fn canonicalize_pattern(pattern: &str) -> String {
    // Split at the deepest path separator before any glob
    // metacharacter. The slash stays with the glob suffix so
    // re-joining doesn't have to insert one back.
    let glob_meta_at = pattern.find(['*', '?', '[', ']', '{', '}']);
    let static_end = match glob_meta_at {
        None => pattern.len(),
        Some(meta) => match pattern[..meta].rfind('/') {
            Some(0) => 1, // pattern like `/*.txt`: keep the root
            Some(slash) => slash,
            None => return pattern.to_string(),
        },
    };
    let (static_part, glob_part) = pattern.split_at(static_end);

    if static_part.is_empty() {
        return pattern.to_string();
    }

    // Walk up from the static portion until we hit an
    // existing ancestor. Components stripped along the way
    // accumulate as the suffix to re-attach after canonicalization.
    let static_path = PathBuf::from(static_part);
    let mut existing = static_path.as_path();
    let mut suffix_components: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            return pattern.to_string();
        };
        suffix_components.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            return pattern.to_string();
        };
        if parent == existing {
            return pattern.to_string();
        }
        existing = parent;
    }

    let Ok(canonical) = std::fs::canonicalize(existing) else {
        return pattern.to_string();
    };

    let mut resolved = canonical;
    for component in suffix_components.iter().rev() {
        resolved.push(component);
    }

    format!("{}{}", resolved.display(), glob_part)
}

/// Reject syntactically invalid request paths. The fs API
/// requires absolute, canonical-shape paths from gadgets —
/// `..` / `.` / `//` segments are refused before any I/O.
pub(crate) fn validate_request_path(path: &str) -> Result<(), WasmFsError> {
    if path.is_empty() {
        return Err(WasmFsError::InvalidPath("empty".into()));
    }
    if path.contains('\0') {
        return Err(WasmFsError::InvalidPath("contains NUL".into()));
    }
    if !Path::new(path).is_absolute() {
        return Err(WasmFsError::InvalidPath(format!(
            "must be absolute: {path}"
        )));
    }
    if path.contains("//") {
        return Err(WasmFsError::InvalidPath(format!(
            "contains `//` segment: {path}"
        )));
    }
    for segment in path.split('/') {
        if segment == ".." || segment == "." {
            return Err(WasmFsError::InvalidPath(format!(
                "contains `{segment}` traversal segment: {path}"
            )));
        }
    }
    Ok(())
}

/// Validate a request path and resolve it through the
/// allowlist gate. Returns the canonicalized real path on
/// success; the variant returned on failure tells the caller
/// whether to surface `not-found`, `permission-denied`, etc.
fn resolve_request(allowlist: Option<&FsAllowlist>, path: &str) -> Result<PathBuf, WasmFsError> {
    validate_request_path(path)?;

    let allowlist = allowlist.ok_or_else(|| WasmFsError::PermissionDenied(path.to_string()))?;

    let canonical = match std::fs::canonicalize(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(WasmFsError::NotFound);
        }
        Err(e) => return Err(WasmFsError::Io(e.to_string())),
    };

    if !allowlist.globset.is_match(&canonical) {
        return Err(WasmFsError::PermissionDenied(
            canonical.display().to_string(),
        ));
    }

    Ok(canonical)
}

impl bindings::torchsnap::gadget::fs::Host for GadgetState {
    fn read_file(
        &mut self,
        path: String,
    ) -> Result<Vec<u8>, bindings::torchsnap::gadget::fs::FsError> {
        use bindings::torchsnap::gadget::fs::FsError as WitFsError;

        let allowlist = self
            .caps
            .as_ref()
            .and_then(|c| c.fs.allowlist.clone());
        tokio::task::block_in_place(|| -> Result<Vec<u8>, WasmFsError> {
            let canonical = resolve_request(allowlist.as_deref(), &path)?;
            std::fs::read(&canonical).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => WasmFsError::NotFound,
                _ => WasmFsError::Io(e.to_string()),
            })
        })
        .map_err(WitFsError::from)
    }

    fn file_exists(&mut self, path: String) -> bool {
        let allowlist = self
            .caps
            .as_ref()
            .and_then(|c| c.fs.allowlist.clone());
        tokio::task::block_in_place(|| resolve_request(allowlist.as_deref(), &path).is_ok())
    }

    fn metadata(
        &mut self,
        path: String,
    ) -> Result<
        bindings::torchsnap::gadget::fs::FileMetadata,
        bindings::torchsnap::gadget::fs::FsError,
    > {
        use bindings::torchsnap::gadget::fs::{FileMetadata, FsError as WitFsError};

        let allowlist = self
            .caps
            .as_ref()
            .and_then(|c| c.fs.allowlist.clone());
        tokio::task::block_in_place(|| -> Result<FileMetadata, WasmFsError> {
            let is_symlink = std::fs::symlink_metadata(&path)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false);

            let canonical = resolve_request(allowlist.as_deref(), &path)?;
            let meta = std::fs::metadata(&canonical).map_err(|e| match e.kind() {
                std::io::ErrorKind::NotFound => WasmFsError::NotFound,
                _ => WasmFsError::Io(e.to_string()),
            })?;

            let modified_unix_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);

            Ok(FileMetadata {
                size: meta.len(),
                modified_unix_ms,
                is_symlink,
            })
        })
        .map_err(WitFsError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;
    use tempfile::TempDir;

    use crate::paths::{GadgetPaths, PlatformPaths};

    fn ctx_for(tmp: &TempDir) -> GadgetPaths {
        let root = tmp.path().to_path_buf();
        GadgetPaths {
            platform: std::sync::Arc::new(PlatformPaths {
                home: root.join("home"),
                xdg_config: root.join("xdg-config"),
                xdg_data: root.join("xdg-data"),
            }),
            gadget_data: root.join("gadget-data"),
            gadget_archive: root.join("gadget-archive"),
        }
    }

    // ─── validate_request_path ─────────────────────────────

    #[test]
    fn validate_request_path_accepts_clean_absolute() {
        validate_request_path("/etc/hosts").expect("clean absolute");
    }

    #[test]
    fn validate_request_path_rejects_empty() {
        match validate_request_path("") {
            Err(WasmFsError::InvalidPath(_)) => {}
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn validate_request_path_rejects_relative() {
        match validate_request_path("etc/hosts") {
            Err(WasmFsError::InvalidPath(_)) => {}
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn validate_request_path_rejects_traversal() {
        for bad in ["/foo/../bar", "/foo/./bar", "/.."] {
            match validate_request_path(bad) {
                Err(WasmFsError::InvalidPath(_)) => {}
                other => panic!("{bad}: expected InvalidPath, got {other:?}"),
            }
        }
    }

    #[test]
    fn validate_request_path_rejects_double_slash() {
        match validate_request_path("/foo//bar") {
            Err(WasmFsError::InvalidPath(_)) => {}
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn validate_request_path_rejects_nul_byte() {
        match validate_request_path("/foo\0bar") {
            Err(WasmFsError::InvalidPath(_)) => {}
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    // ─── canonicalize_pattern ──────────────────────────────

    #[test]
    fn canonicalize_pattern_resolves_existing_path() {
        let tmp = TempDir::new().expect("tempdir");
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).expect("mkdir");
        let link = tmp.path().join("link");
        symlink(&real, &link).expect("symlink");
        std::fs::write(real.join("file.txt"), b"content").expect("write");

        let pattern = format!("{}/file.txt", link.display());
        let canonical = canonicalize_pattern(&pattern);
        let real_canonical = std::fs::canonicalize(&real).expect("canonicalize real");
        assert_eq!(canonical, format!("{}/file.txt", real_canonical.display()));
    }

    #[test]
    fn canonicalize_pattern_preserves_glob_suffix() {
        let tmp = TempDir::new().expect("tempdir");
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).expect("mkdir");

        let pattern = format!("{}/*.txt", real.display());
        let canonical = canonicalize_pattern(&pattern);
        let real_canonical = std::fs::canonicalize(&real).expect("canonicalize real");
        assert_eq!(canonical, format!("{}/*.txt", real_canonical.display()));
    }

    #[test]
    fn canonicalize_pattern_handles_nonexistent_leaf() {
        let tmp = TempDir::new().expect("tempdir");
        let real = tmp.path().join("real");
        std::fs::create_dir(&real).expect("mkdir");

        // File doesn't exist yet — the deepest existing ancestor is `real`.
        let pattern = format!("{}/missing.txt", real.display());
        let canonical = canonicalize_pattern(&pattern);
        let real_canonical = std::fs::canonicalize(&real).expect("canonicalize real");
        assert_eq!(
            canonical,
            format!("{}/missing.txt", real_canonical.display())
        );
    }

    #[test]
    fn canonicalize_pattern_returns_unchanged_on_unresolvable() {
        let pattern = "/this/path/cannot/exist/anywhere/sane/file.txt";
        let canonical = canonicalize_pattern(pattern);
        // Best effort: at minimum the input survives; if `/` happens to
        // canonicalize the result still contains the unchanged suffix.
        assert!(canonical.ends_with("/this/path/cannot/exist/anywhere/sane/file.txt"));
    }

    // ─── compile_fs_patterns ───────────────────────────────

    #[test]
    fn compile_fs_patterns_substitutes_variables() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        std::fs::create_dir_all(&ctx.platform.xdg_config).expect("mkdir xdg-config");
        let target = ctx.platform.xdg_config.join("config.toml");
        std::fs::write(&target, b"x").expect("write");

        let allowlist =
            compile_fs_patterns(&["${xdg-config}/config.toml".into()], &ctx).expect("compile");

        let canonical_target = std::fs::canonicalize(&target).expect("canonicalize");
        assert!(allowlist.globset.is_match(&canonical_target));
    }

    #[test]
    fn compile_fs_patterns_rejects_unknown_variable() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        let err = compile_fs_patterns(&["${nope}/x".into()], &ctx).expect_err("should fail");
        assert!(err.to_string().contains("substitution failed"));
    }

    #[test]
    fn compile_fs_patterns_rejects_unsupported_glob_metacharacter() {
        // Brace alternation, `?`, and character classes are
        // not part of the supported glob surface — only `*`
        // and `**`. The check runs after substitution so a
        // legitimate `${xdg-config}` token never trips it.
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        for pattern in ["/etc/{a,b}.conf", "/etc/h?st", "/etc/[abc].conf"] {
            let err = compile_fs_patterns(&[pattern.into()], &ctx)
                .expect_err(&format!("expected rejection for `{pattern}`"));
            assert!(
                err.to_string().contains("unsupported glob metacharacter"),
                "pattern `{pattern}`: {err}"
            );
        }
    }

    #[test]
    fn compile_fs_patterns_accepts_substitution_token_braces() {
        // Regression: a `${xdg-config}` token must not trip
        // the metacharacter check — the braces there belong
        // to the substitution syntax, and after substitution
        // they're gone entirely.
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        std::fs::create_dir_all(&ctx.platform.xdg_config).expect("mkdir xdg-config");
        compile_fs_patterns(
            &["${xdg-config}/ZeroTier/One/authtoken.secret".into()],
            &ctx,
        )
        .expect("compile should succeed for substitution-only pattern");
    }

    // ─── resolve_request ───────────────────────────────────

    #[test]
    fn resolve_request_accepts_allowed_existing_path() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        std::fs::create_dir_all(&ctx.platform.xdg_config).expect("mkdir");
        let path = ctx.platform.xdg_config.join("ok.txt");
        std::fs::write(&path, b"hello").expect("write");

        let allowlist =
            compile_fs_patterns(&["${xdg-config}/*.txt".into()], &ctx).expect("compile");

        let canonical = resolve_request(Some(&allowlist), path.to_str().unwrap()).expect("allowed");
        assert_eq!(canonical, std::fs::canonicalize(&path).unwrap());
    }

    #[test]
    fn resolve_request_denies_path_outside_allowlist() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        std::fs::create_dir_all(&ctx.platform.xdg_config).expect("mkdir xdg");
        std::fs::create_dir_all(&ctx.platform.home).expect("mkdir home");
        let outside = ctx.platform.home.join("secret.txt");
        std::fs::write(&outside, b"nope").expect("write");

        let allowlist =
            compile_fs_patterns(&["${xdg-config}/*.txt".into()], &ctx).expect("compile");

        match resolve_request(Some(&allowlist), outside.to_str().unwrap()) {
            Err(WasmFsError::PermissionDenied(_)) => {}
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn resolve_request_returns_not_found_for_missing_path() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        std::fs::create_dir_all(&ctx.platform.xdg_config).expect("mkdir");
        let allowlist =
            compile_fs_patterns(&["${xdg-config}/*.txt".into()], &ctx).expect("compile");

        let missing = ctx.platform.xdg_config.join("missing.txt");
        match resolve_request(Some(&allowlist), missing.to_str().unwrap()) {
            Err(WasmFsError::NotFound) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn resolve_request_denies_when_no_allowlist() {
        match resolve_request(None, "/etc/hosts") {
            Err(WasmFsError::PermissionDenied(_)) => {}
            other => panic!("expected PermissionDenied, got {other:?}"),
        }
    }

    #[test]
    fn resolve_request_follows_symlink_target_through_allowlist() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        std::fs::create_dir_all(&ctx.platform.xdg_config).expect("mkdir xdg");
        std::fs::create_dir_all(&ctx.platform.home).expect("mkdir home");

        // Real file lives inside xdg_config; a symlink in xdg_config
        // points to it. Both source and target are inside the
        // allowlist so the read should succeed.
        let real = ctx.platform.xdg_config.join("real.txt");
        std::fs::write(&real, b"contents").expect("write");
        let link = ctx.platform.xdg_config.join("link.txt");
        symlink(&real, &link).expect("symlink");

        let allowlist =
            compile_fs_patterns(&["${xdg-config}/*.txt".into()], &ctx).expect("compile");

        resolve_request(Some(&allowlist), link.to_str().unwrap()).expect("symlink-to-allowed");
    }

    #[test]
    fn resolve_request_denies_symlink_pointing_outside_allowlist() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        std::fs::create_dir_all(&ctx.platform.xdg_config).expect("mkdir xdg");
        std::fs::create_dir_all(&ctx.platform.home).expect("mkdir home");

        // Real file lives in `home` (not in allowlist); a symlink in
        // xdg_config (which IS in allowlist) points to it. Resolving
        // through the symlink to the real path must fail because
        // the real path is outside the allowlist.
        let real = ctx.platform.home.join("escape.txt");
        std::fs::write(&real, b"escape").expect("write");
        let link = ctx.platform.xdg_config.join("link.txt");
        symlink(&real, &link).expect("symlink");

        let allowlist =
            compile_fs_patterns(&["${xdg-config}/*.txt".into()], &ctx).expect("compile");

        match resolve_request(Some(&allowlist), link.to_str().unwrap()) {
            Err(WasmFsError::PermissionDenied(_)) => {}
            other => panic!("expected PermissionDenied (symlink escape), got {other:?}"),
        }
    }

    #[test]
    fn resolve_request_rejects_traversal_in_request() {
        let tmp = TempDir::new().expect("tempdir");
        let ctx = ctx_for(&tmp);
        let allowlist =
            compile_fs_patterns(&["${xdg-config}/*.txt".into()], &ctx).expect("compile");

        match resolve_request(Some(&allowlist), "/etc/../etc/hosts") {
            Err(WasmFsError::InvalidPath(_)) => {}
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }
}
