# Clipboard gadget: enable() panics on DB failure, state is never cleared on disable, handle_message panics before enable completes

**Kind:** bug
**Severity:** high
**Area:** src-tauri/src/gadgets/clipboard/mod.rs

## Problem

Three related lifecycle defects around `ClipboardGadget::state`:

1. **`enable()` panics instead of returning `Err`.**
   `src-tauri/src/gadgets/clipboard/mod.rs:303`:
   ```rust
   let sql = SqlStorage::open(db_path, &[MIGRATION_001]).expect("open clipboard database");
   ```
   Opening the database can genuinely fail (corrupt file, disk
   full, permissions, failed migration). The `Gadget::enable`
   contract handles this case gracefully: "Returning `Err`
   disables the gadget — no further calls dispatched"
   (`src-tauri/src/gadgets/mod.rs`, trait doc). Instead the
   `expect` panics on the `spawn_blocking` enable thread. The
   project convention also requires `expect` only where failure
   is impossible, with the message explaining why — not the case
   here.

2. **`disable()` never clears the state, contradicting its own
   documentation.** The field doc says: "Shared state (DB + file
   storage). Initialized in `enable()`, cleared in `disable()`"
   (`mod.rs:90-92`). `disable()` (`mod.rs:340-347`) only flips
   lifecycle flags and stops the watcher; `self.state` keeps the
   `Arc<SharedState>`, so the SQLite connection and file storage
   stay open for a disabled gadget, and `handle_message` continues
   to operate on them if a custom-UI view is still open.

3. **`state()` panics when called before `enable()` has
   completed.** `mod.rs:134-141`:
   ```rust
   self.state.lock().expect("state not poisoned")
       .as_ref().expect("clipboard state initialized").clone()
   ```
   `handle_message` calls `self.state()` unconditionally
   (`mod.rs:407`). `enable()` runs on a background thread; the
   clipboard shortcut is registered independently and
   `handle_shortcut` opens the custom UI without touching state
   (`mod.rs:279-287`). A user pressing the clipboard shortcut
   right after startup can have the UI issue a `search` message
   before `enable()` has stored the state → panic in the message
   handler. The same applies if messages arrive for a
   never-enabled gadget.

## Impact

- DB corruption or disk issues crash a background thread at
  startup instead of cleanly disabling the gadget.
- Disabling the clipboard gadget does not actually release its
  database; a still-open history view keeps working against a
  "disabled" gadget.
- Startup race panics the message-handler path.

## Suggested fix

- `enable()`: propagate the error (`.context("open clipboard
  database")?`) so the host's enable-failure path handles it.
  (Related: `todos/architecture/01kr9j552samx34befphthjp5p-enable-failure-not-reflected-in-store.md`.)
- `disable()`: take the `Arc` out of `self.state` (drop it) so
  the DB closes once in-flight operations finish, matching the
  field documentation.
- `state()`: return `anyhow::Result<Arc<SharedState>>` ("clipboard
  gadget not enabled") and propagate through `handle_message` so
  early/late messages produce an error response instead of a
  panic.
