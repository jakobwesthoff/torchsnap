---
title: "Gadget discovery silently swallows I/O errors"
kind: improvement
component: gadget-host
impact: low
---
# Gadget discovery silently swallows I/O errors

Gadget discovery treats every failure to read a gadget root as an empty root, and it drops per-entry errors without logging. If `<app_data_dir>/gadgets/` becomes unreadable, all user gadgets vanish with no log line explaining why.

## Problem
`scan_gadget_entries` in `src-tauri/src/wasm/discovery.rs` (around lines 108 to 111) treats every `read_dir` failure as an empty root:

```rust
let read_dir = match std::fs::read_dir(root) {
    Ok(d) => d,
    Err(_) => return Vec::new(),
};
```

It also drops per-entry errors via `read_dir.flatten()` (around line 119). The doc comment justifies this for a missing root. The same silence applies to genuinely abnormal failures: permission denied on the user gadgets directory, or transient I/O errors on individual entries. Nothing is logged in either case.

## Impact
If `<app_data_dir>/gadgets/` becomes unreadable (wrong permissions after a backup restore, sandboxing change), all user gadgets simply vanish from the app with no log line to explain why.

## Proposal
Distinguish `ErrorKind::NotFound` (expected, stay silent) from other errors (log a warning with the root path and error). Do the same for per-entry errors: replace `.flatten()` with a loop that logs failures. The return type can stay `Vec<PathBuf>`.
