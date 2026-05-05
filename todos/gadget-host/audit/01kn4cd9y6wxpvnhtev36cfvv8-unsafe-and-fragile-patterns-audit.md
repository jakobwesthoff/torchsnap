# Unsafe and fragile patterns audit

Codebase audit for remaining unsafe blocks, fragile pointer patterns,
and unrecoverable panics in production code paths. Three items warrant
attention.

## 1. Unchecked integer overflow in `cgimage_conversion.rs:91`

`slice::from_raw_parts` uses `height * bytes_per_row` to compute
the buffer length. For extremely large images this `usize` product
can overflow, producing a wildly incorrect slice length and UB.

```rust
let premultiplied = unsafe {
    std::slice::from_raw_parts(data_ptr as *const u8, total_bytes)
};
```

**Fix:** Replace the multiplication with
`height.checked_mul(bytes_per_row).context("image too large")?`
before passing it to `from_raw_parts`.

## 2. `MainThreadMarker::new_unchecked` in `launcher_panel.rs:156`

Asserts the current thread is main without runtime verification.
The call site is inside `run_on_main_thread`, which Tauri guarantees
dispatches to the main thread — but if this code is ever moved or
refactored, the marker becomes silently invalid and causes UB in
ObjC dispatch.

```rust
let mtm = unsafe { MainThreadMarker::new_unchecked() };
```

**Fix:** Prefer `MainThreadMarker::new()` (returns `Option`, panics
with a clear message on wrong thread), or add a
`debug_assert!(MainThreadMarker::new().is_some())` before the
unsafe block.

## 3. `panic!` in `ToSql` impl — `sql_storage.rs:119`

`SqlValue::List(_)` hits a `panic!` inside `to_sql()`, which is
called by rusqlite during query execution. The caller has no way to
catch this through normal error handling — it crashes the thread.

```rust
SqlValue::List(_) => panic!("List values must be expanded before binding"),
```

**Fix:** Return an `Err` instead:
```rust
SqlValue::List(_) => Err(rusqlite::Error::ToSqlConversionFailure(
    "List values must be expanded before binding".into(),
))
```

## Not included (acceptable)

- **`lib.rs:172`** (`*mut c_void → &NSWindow`): Well-commented,
  unavoidable given Tauri's API surface. Brief borrow, sound lifetime.
- **`cgimage_conversion.rs:59,143`**: Unavoidable Core Graphics FFI,
  well-commented safety blocks.
- **Detached `thread::spawn`** (clipboard, plugin_host): All have
  proper shutdown coordination via condvars/channels. The paste
  thread (`clipboard/mod.rs:521`) is fire-and-forget by design.
  (The native calculator's retention thread was deleted in the
  WASM port and replaced by the host scheduler — see ADR 0032.)
- **`.unwrap()` in test code**: Standard test practice in the
  remaining native plugins, not a production concern.
