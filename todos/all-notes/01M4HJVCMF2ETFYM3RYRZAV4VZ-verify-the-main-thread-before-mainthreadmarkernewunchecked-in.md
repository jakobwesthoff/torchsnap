---
title: "Verify the main thread before MainThreadMarker::new_unchecked in launcher_panel"
kind: bug
component: platform
origin: review
tags: [security, macos]
---
# Verify the main thread before MainThreadMarker::new_unchecked in launcher_panel

Found in the unsafe and fragile patterns audit: a codebase audit for remaining unsafe blocks, fragile pointer patterns, and unrecoverable panics in production code paths. This is one of three items that audit flagged; the other two are filed as separate todos. The audit cited `src-tauri/src/platform/macos/launcher_panel.rs:156`; the call is now around line 148.

## Problem

`MainThreadMarker::new_unchecked` asserts the current thread is main without runtime verification.
The call site is inside `run_on_main_thread`, which Tauri guarantees
dispatches to the main thread. If this code is ever moved or
refactored, the marker becomes silently invalid and causes UB in
ObjC dispatch.

```rust
let mtm = unsafe { MainThreadMarker::new_unchecked() };
```

## Suggested fix

Prefer `MainThreadMarker::new()` (returns `Option`, panics
with a clear message on wrong thread), or add a
`debug_assert!(MainThreadMarker::new().is_some())` before the
unsafe block.
