# Gadget discovery silently swallows I/O errors

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/wasm/discovery.rs

## Problem
`scan_gadget_entries` treats every `read_dir` failure as an empty
root (`src-tauri/src/wasm/discovery.rs:108-111`):

```rust
let read_dir = match std::fs::read_dir(root) {
    Ok(d) => d,
    Err(_) => return Vec::new(),
};
```

and drops per-entry errors via `read_dir.flatten()` (`:119`). The
doc comment (`:94-95`) justifies this for a missing root, but the
same silence applies to genuinely abnormal failures: permission
denied on the user gadgets directory, or transient I/O errors on
individual entries. Nothing is logged in either case.

## Impact
If `<app_data_dir>/gadgets/` becomes unreadable (wrong
permissions after a backup restore, sandboxing change), all user
gadgets simply vanish from the app with no log line to explain
why.

## Suggested fix
Distinguish `ErrorKind::NotFound` (expected, stay silent) from
other errors (log a warning with the root path and error). Same
for per-entry errors: replace `.flatten()` with a loop that logs
failures. Return type can stay `Vec<PathBuf>`.
