# FilesystemCap::metadata does I/O before path validation and swallows symlink-check errors

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/caps/filesystem.rs

## Problem
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
   when validation fails, so nothing leaks — but the read-file
   and exists paths validate *before* touching the filesystem,
   and this one silently deviates from that pattern.
2. **Swallowed error.** `unwrap_or(false)` maps any
   `symlink_metadata` failure (EACCES on a parent dir, I/O
   error) to "not a symlink", so callers can receive
   `is_symlink: false` for a path that actually is a symlink.
3. **Split-read inconsistency.** `is_symlink` describes the path
   at time T1 while `size`/`modified_unix_ms` come from the
   canonical target at time T2 (`filesystem.rs:141-151`); a
   concurrent change between the two syscalls produces a
   `FileMetadata` that never existed. Harmless for current
   consumers, but undocumented.

## Impact
No permission bypass. Wrong `is_symlink` values under permission
errors, and metadata assembled from two non-atomic reads. Mostly
a robustness/consistency cleanup at a permission-sensitive
boundary where the code should be boringly uniform.

## Suggested fix
Run `resolve_request` first, then compute `is_symlink` (a path
that fails validation or the allowlist never reaches any
syscall). Propagate `symlink_metadata` errors as
`FilesystemError::Io` instead of defaulting to `false`, or
document why "unknown" collapses to `false`.
