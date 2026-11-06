---
title: "Filesystem metadata runs I/O before path validation and swallows symlink-check errors"
kind: bug
component: gadget-host
impact: low
tags: [capabilities]
---
# Filesystem metadata runs I/O before path validation and swallows symlink-check errors

`FilesystemCap::metadata` makes a syscall on the raw request path before validating it, and it maps any failure of that syscall to "not a symlink". The result is a robustness and consistency gap at a permission-sensitive boundary, not a permission bypass.

## Problem
Read in the code, not reproduced.

`FilesystemCap::metadata` starts with a `symlink_metadata` call
on the raw, still-unvalidated request path
(`src-tauri/src/caps/filesystem.rs:134-140`):

```rust
pub fn metadata(&self, path: &str) -> Result<FileMetadata, FilesystemError> {
    tokio::task::block_in_place(|| {
        let is_symlink = std::fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);

        let canonical = resolve_request(&self.allowlist, path)?;
        ...
```

Three issues in that ordering:

1. **I/O before validation.** `validate_request_path` (inside
   `resolve_request`) is the designated gate for malformed input
   (empty, NUL, relative, traversal), yet a syscall on the
   gadget-supplied string happens first. The result is discarded
   when validation fails, so nothing leaks. The read-file
   and exists paths validate *before* touching the filesystem,
   and this one silently deviates from that pattern.
2. **Swallowed error.** `unwrap_or(false)` maps any
   `symlink_metadata` failure (EACCES on a parent dir, I/O
   error) to "not a symlink", so callers can receive
   `is_symlink: false` for a path that actually is a symlink.
3. **Split-read inconsistency.** `is_symlink` describes the path
   at time T1 while `size`/`modified_unix_ms` come from the
   canonical target at time T2 (`filesystem.rs:141-151`). A
   concurrent change between the two syscalls produces a
   `FileMetadata` that never existed. This is harmless for current
   consumers, but undocumented.

## Impact
No permission bypass. Wrong `is_symlink` values under permission
errors, and metadata assembled from two non-atomic reads. Mostly
a robustness and consistency cleanup at a permission-sensitive
boundary where the code should be boringly uniform.

## Suggested fix
Run `resolve_request` first, then compute `is_symlink`. A path
that fails validation or the allowlist then never reaches any
syscall. Propagate `symlink_metadata` errors as
`FilesystemError::Io` instead of defaulting to `false`, or
document why "unknown" collapses to `false`.
