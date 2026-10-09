---
kind: improvement
severity: low
status: open
area: [src-tauri/src/platform/macos/clipboard.rs]
---

# Clipboard self-write marker: module comment names the wrong pasteboard type

## Problem

The module header comment says the self-write marker type is
`com.torchsnap.clipboard-self-write`
(`src-tauri/src/platform/macos/clipboard.rs:12`):

```
// - Self-write marker (`com.torchsnap.clipboard-self-write`):
```

The actual constant is `app.torchsnap.clipboard-self-write`
(`clipboard.rs:28`):

```rust
const SELF_WRITE_TYPE: &str = "app.torchsnap.clipboard-self-write";
```

`com.` vs `app.` reverse-DNS prefix. Anyone debugging pasteboard
contents (e.g. with a pasteboard viewer) and searching for the
commented type will not find it.

## Suggested fix

Update the comment to `app.torchsnap.clipboard-self-write`, or
better, drop the literal from the comment and reference the
`SELF_WRITE_TYPE` constant so the two cannot drift again.
