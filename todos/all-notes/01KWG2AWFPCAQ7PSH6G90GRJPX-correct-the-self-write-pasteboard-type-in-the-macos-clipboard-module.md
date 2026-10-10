---
title: "Correct the self-write pasteboard type in the macOS clipboard module comment"
kind: chore
component: platform
impact: low
---
# Correct the self-write pasteboard type in the macOS clipboard module comment

The module header comment in `src-tauri/src/platform/macos/clipboard.rs`
names the self-write marker type with the wrong reverse-DNS prefix, so
anyone searching pasteboard contents for the commented type will not find
it.

## Problem

The module header comment says the self-write marker type is
`com.torchsnap.clipboard-self-write` (line 12):

```
// - Self-write marker (`com.torchsnap.clipboard-self-write`):
```

The actual constant `SELF_WRITE_TYPE` (around line 28) is:

```rust
const SELF_WRITE_TYPE: &str = "app.torchsnap.clipboard-self-write";
```

The comment uses `com.` where the constant uses `app.`. Anyone debugging
pasteboard contents (e.g. with a pasteboard viewer) and searching for the
commented type will not find it.

## Suggested fix

Update the comment to `app.torchsnap.clipboard-self-write`, or
better, drop the literal from the comment and reference the
`SELF_WRITE_TYPE` constant so the two cannot drift again.
