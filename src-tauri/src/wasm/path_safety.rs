// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Path Safety
//
// Shared helper for "is this candidate path inside this
// declared root?" checks. Used by the argv matcher's
// `path-under` constraint, by per-rule cwd validation in the
// manifest parser, and (future) by bundled-executable path
// resolution.
//
// The check supports candidates that do not yet exist: the
// deepest existing ancestor is canonicalized so symlinks
// along that chain cannot be used to escape, and the
// remaining lexical tail is appended to the canonical
// ancestor. The lexical pre-pass strips `.` and `..`
// components so the tail can never escape upward. A symlink
// counts as existing even when its target does not, so a
// dangling link is never skipped as part of the tail; it is
// rejected instead.
//
// Distinct from the lexical-only `validate_gadget_path`
// helper in `source.rs`, which solves a different problem
// (manifest-relative paths inside an archive — no fs touch
// allowed). The two coexist.
// =========================================================

use std::path::{Component, Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PathError {
    /// `candidate` was a relative path. Callers must pass
    /// absolute paths so canonicalization does not silently
    /// resolve against the host's cwd.
    #[error("candidate path is not absolute: {0}")]
    CandidateNotAbsolute(PathBuf),

    /// `root` was a relative path. Same rationale as above.
    #[error("root path is not absolute: {0}")]
    RootNotAbsolute(PathBuf),

    /// `root` could not be canonicalized — typically because
    /// the directory does not exist. The check is meaningless
    /// without a real root.
    #[error("root path `{root}` could not be canonicalized: {source}")]
    RootCanonicalize {
        root: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The deepest existing ancestor of the candidate could
    /// not be canonicalized (filesystem error other than the
    /// non-existence we walk past). Distinct from
    /// `EscapesRoot` so callers can distinguish "I/O failed"
    /// from "this path is outside its declared root".
    #[error("candidate path canonicalization failed: {0}")]
    CandidateCanonicalize(std::io::Error),

    /// A symlink on the candidate path points at something that
    /// does not exist. Its target cannot be canonicalized and
    /// checked against the root, and writing to the path would
    /// create the target wherever the link points, so the path is
    /// rejected whatever the target. Carries the link's path.
    #[error("path `{0}` goes through a symlink whose target does not exist")]
    DanglingSymlink(PathBuf),

    /// The canonicalized candidate does not lie under the
    /// canonicalized root. The actual resolved path is
    /// included to make manifest-time errors actionable.
    #[error("path `{resolved}` escapes its declared root")]
    EscapesRoot { resolved: PathBuf },
}

/// Resolve `candidate` against `root` and confirm it lies
/// inside the canonicalized root. Returns the canonical
/// candidate path.
///
/// Algorithm:
///
/// 1. Both inputs must be absolute. Relative paths would
///    canonicalize against the host cwd, which has no
///    meaningful relationship to the gadget's view of the
///    filesystem.
/// 2. Lexically resolve `.` and `..` in the candidate
///    (purely string-level, no fs touch). This means a
///    candidate like `/a/b/../c` becomes `/a/c` even if
///    `/a/b` does not exist.
/// 3. Canonicalize the root. The root is required to exist
///    — the check is meaningless if it does not.
/// 4. Walk up the candidate to find the deepest existing
///    ancestor, where a symlink exists even if its target
///    does not. Canonicalize that; a dangling symlink fails
///    here and is reported as `DanglingSymlink`. Re-attach
///    the still-unresolved tail (which after step 2 contains
///    only plain `Normal` components, so it cannot escape).
/// 5. Compare the canonical resolved path against the
///    canonical root via `starts_with`.
///
/// Supports candidates that do not exist: only the
/// deepest existing ancestor is canonicalized; the
/// remaining tail is treated lexically. This matches the
/// security guarantee plain `fs::canonicalize` would give
/// us for existing files and extends it to allow the
/// "scaffold-this-path-later" case used by future
/// bundled-binary support.
pub fn canonical_under_root(candidate: &Path, root: &Path) -> Result<PathBuf, PathError> {
    if !candidate.is_absolute() {
        return Err(PathError::CandidateNotAbsolute(candidate.to_path_buf()));
    }
    if !root.is_absolute() {
        return Err(PathError::RootNotAbsolute(root.to_path_buf()));
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|source| PathError::RootCanonicalize {
            root: root.to_path_buf(),
            source,
        })?;

    let lexical = lexically_normalize(candidate);
    let (existing_ancestor, tail) = split_at_existing_ancestor(&lexical);

    let canonical_existing = existing_ancestor.canonicalize().map_err(|source| {
        let is_symlink = existing_ancestor
            .symlink_metadata()
            .is_ok_and(|meta| meta.file_type().is_symlink());
        if is_symlink && source.kind() == std::io::ErrorKind::NotFound {
            PathError::DanglingSymlink(existing_ancestor.clone())
        } else {
            PathError::CandidateCanonicalize(source)
        }
    })?;

    let resolved = if tail.as_os_str().is_empty() {
        canonical_existing
    } else {
        canonical_existing.join(&tail)
    };

    if !resolved.starts_with(&canonical_root) {
        return Err(PathError::EscapesRoot { resolved });
    }

    Ok(resolved)
}

/// Lexical resolution of `.` and `..` components. Operates
/// purely on the path string — does not consult the
/// filesystem. The result still contains the same prefix
/// kind (root / drive prefix) as the input.
///
/// `..` consumes the previous `Normal` component; if the
/// path begins with `..` (relative path) the component is
/// preserved. We're called only on absolute paths so this
/// degenerate case is never reached in practice, but the
/// implementation stays conservative.
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push(Component::ParentDir);
                }
            }
            other => result.push(other),
        }
    }
    result
}

/// Walk up `path` until an existing ancestor is found.
/// Returns `(existing_ancestor, tail)` such that
/// `existing_ancestor.join(&tail)` reproduces `path`. If
/// the full path exists, `tail` is empty.
///
/// Existence is checked without following symlinks: a
/// dangling symlink is an existing entry, so it ends the walk
/// and reaches canonicalization instead of hiding in the tail.
///
/// For absolute paths the loop is guaranteed to terminate
/// at the root component (which always exists on every
/// supported platform).
fn split_at_existing_ancestor(path: &Path) -> (PathBuf, PathBuf) {
    let mut existing = path.to_path_buf();
    let mut tail = PathBuf::new();

    while existing.symlink_metadata().is_err() {
        let Some(name) = existing.file_name().map(|n| n.to_owned()) else {
            // Reached root or a path that has no file name
            // (e.g. `/` itself). Stop walking; tail carries
            // whatever we've accumulated.
            break;
        };
        let parent = existing
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_default();

        tail = if tail.as_os_str().is_empty() {
            PathBuf::from(name)
        } else {
            let mut combined = PathBuf::from(name);
            combined.push(&tail);
            combined
        };
        existing = parent;
    }

    (existing, tail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn td() -> TempDir {
        TempDir::new().expect("tempdir")
    }

    #[test]
    fn rejects_relative_candidate() {
        let root = td();
        let err = canonical_under_root(Path::new("foo/bar"), root.path()).unwrap_err();
        assert!(matches!(err, PathError::CandidateNotAbsolute(_)));
    }

    #[test]
    fn rejects_relative_root() {
        let err = canonical_under_root(Path::new("/a/b"), Path::new("relative")).unwrap_err();
        assert!(matches!(err, PathError::RootNotAbsolute(_)));
    }

    #[test]
    fn rejects_nonexistent_root() {
        let candidate = Path::new("/tmp/whatever");
        let err = canonical_under_root(candidate, Path::new("/this/does/not/exist/anywhere"))
            .unwrap_err();
        assert!(matches!(err, PathError::RootCanonicalize { .. }));
    }

    #[test]
    fn accepts_existing_file_inside_root() {
        let root = td();
        let file = root.path().join("inside.txt");
        fs::write(&file, b"hi").expect("write");

        let resolved = canonical_under_root(&file, root.path()).expect("inside root");
        assert!(resolved.starts_with(root.path().canonicalize().expect("canonical root")));
    }

    #[test]
    fn accepts_nonexistent_file_with_existing_parent() {
        let root = td();
        let absent = root.path().join("not-yet-created.txt");

        let resolved = canonical_under_root(&absent, root.path()).expect("inside root");
        assert!(resolved.starts_with(root.path().canonicalize().expect("canonical root")));
        assert!(resolved.ends_with("not-yet-created.txt"));
    }

    #[test]
    fn accepts_nonexistent_nested_path() {
        let root = td();
        let deep = root.path().join("a/b/c/d.txt");

        let resolved = canonical_under_root(&deep, root.path()).expect("inside root");
        assert!(resolved.starts_with(root.path().canonicalize().expect("canonical root")));
        assert!(resolved.ends_with("a/b/c/d.txt"));
    }

    #[test]
    fn rejects_lexical_traversal_escape() {
        let root = td();
        // `<root>/sub/../../escape` lexically resolves to a
        // sibling of `<root>` — outside the root.
        let traversed = root.path().join("sub").join("..").join("..").join("escape");

        let err = canonical_under_root(&traversed, root.path()).unwrap_err();
        assert!(matches!(err, PathError::EscapesRoot { .. }));
    }

    #[test]
    fn accepts_lexical_traversal_back_into_root() {
        let root = td();
        fs::create_dir(root.path().join("sub")).expect("mkdir");
        // `<root>/sub/../inside` resolves back to `<root>/inside`
        // — still under the root.
        let traversed = root.path().join("sub").join("..").join("inside");

        let resolved = canonical_under_root(&traversed, root.path()).expect("inside root");
        assert!(resolved.starts_with(root.path().canonicalize().expect("canonical root")));
        assert!(resolved.ends_with("inside"));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let outside = td();
        let outside_target = outside.path().join("secret.txt");
        fs::write(&outside_target, b"oops").expect("write");

        let root = td();
        let link = root.path().join("trapdoor");
        symlink(&outside_target, &link).expect("symlink");

        // The candidate resolves through a symlink chain that
        // points outside the root. Canonicalization follows
        // the symlink; the post-canonical comparison must
        // catch the escape.
        let err = canonical_under_root(&link, root.path()).unwrap_err();
        assert!(matches!(err, PathError::EscapesRoot { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn accepts_symlink_inside_root() {
        use std::os::unix::fs::symlink;

        let root = td();
        let real = root.path().join("real.txt");
        fs::write(&real, b"hi").expect("write");

        let link = root.path().join("alias.txt");
        symlink(&real, &link).expect("symlink");

        let resolved = canonical_under_root(&link, root.path()).expect("inside root");
        assert!(resolved.starts_with(root.path().canonicalize().expect("canonical root")));
    }

    // A symlink whose target does not exist cannot be canonicalized,
    // so its target is never checked against the root. Writing to the
    // path would create the target wherever the link points.
    #[cfg(unix)]
    #[test]
    fn rejects_dangling_symlink_pointing_outside_the_root() {
        use std::os::unix::fs::symlink;

        let outside = td();
        let root = td();
        let link = root.path().join("trapdoor");
        symlink(outside.path().join("not-yet-there"), &link).expect("symlink");

        let err = canonical_under_root(&link, root.path()).unwrap_err();
        assert!(matches!(err, PathError::DanglingSymlink(ref p) if p == &link));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_dangling_symlink_pointing_inside_the_root() {
        use std::os::unix::fs::symlink;

        let root = td();
        let link = root.path().join("alias");
        symlink(root.path().join("not-yet-there"), &link).expect("symlink");

        let err = canonical_under_root(&link, root.path()).unwrap_err();
        assert!(matches!(err, PathError::DanglingSymlink(_)));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_path_below_a_dangling_symlinked_directory() {
        use std::os::unix::fs::symlink;

        let outside = td();
        let root = td();
        let link = root.path().join("trapdoor");
        symlink(outside.path().join("missing-dir"), &link).expect("symlink");

        let below = link.join("sub").join("file.txt");
        let err = canonical_under_root(&below, root.path()).unwrap_err();
        assert!(matches!(err, PathError::DanglingSymlink(ref p) if p == &link));
    }

    #[cfg(unix)]
    #[test]
    fn rejects_a_symlink_chain_that_ends_dangling() {
        use std::os::unix::fs::symlink;

        let outside = td();
        let root = td();
        let last = root.path().join("last");
        symlink(outside.path().join("not-yet-there"), &last).expect("symlink");
        let first = root.path().join("first");
        symlink(&last, &first).expect("symlink");

        let err = canonical_under_root(&first, root.path()).unwrap_err();
        assert!(matches!(err, PathError::DanglingSymlink(_)));
    }

    #[test]
    fn root_equals_candidate_is_accepted() {
        let root = td();
        let resolved = canonical_under_root(root.path(), root.path()).expect("self");
        assert_eq!(
            resolved,
            root.path().canonicalize().expect("canonical root")
        );
    }
}
