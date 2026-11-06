---
title: "Check the buffer length product in cgimage_conversion for overflow"
kind: bug
component: platform
origin: review
tags: [security, macos]
---
# Check the buffer length product in cgimage_conversion for overflow

Found in the unsafe and fragile patterns audit: a codebase audit for remaining unsafe blocks, fragile pointer patterns, and unrecoverable panics in production code paths. This is one of three items that audit flagged; the other two are filed as separate todos. The audit cited `src-tauri/src/platform/macos/cgimage_conversion.rs:91`; the line number has drifted since.

## Problem

`slice::from_raw_parts` uses `height * bytes_per_row` to compute
the buffer length. For extremely large images this `usize` product
can overflow, producing a wildly incorrect slice length and UB.

```rust
let premultiplied = unsafe {
    std::slice::from_raw_parts(data_ptr as *const u8, total_bytes)
};
```

## Suggested fix

Replace the multiplication with
`height.checked_mul(bytes_per_row).context("image too large")?`
before passing it to `from_raw_parts`.

## Not flagged by the audit (acceptable)

- **`cgimage_conversion.rs:59,143`**: Unavoidable Core Graphics FFI,
  well-commented safety blocks.
- **`lib.rs:172`** (`*mut c_void → &NSWindow`): Well-commented,
  unavoidable given Tauri's API surface. Brief borrow, sound lifetime.
- **Detached `thread::spawn`** (clipboard, gadget_host): All have
  proper shutdown coordination via condvars/channels. The paste
  thread (`clipboard/mod.rs:521`) is fire-and-forget by design.
  (The native calculator's retention thread was deleted in the
  WASM port and replaced by the host scheduler, see ADR 0032.)
- **`.unwrap()` in test code**: Standard test practice in the
  remaining native gadgets, not a production concern.
