# Clipboard gadget init panics on setup failure

## What happens

Observed on Fedora 43 / GNOME Wayland after a log-out/in cycle where
the shell's `XAUTHORITY` was left pointing at a stale
`.mutter-Xwaylandauth.*` cookie file. `ClipboardContext::new()` inside
the `clipboard-rs` crate fails to open an X11 connection and returns
`Err(SetupFailed(..))`. The gadget does:

```rust
ClipboardContext::new().expect("clipboard context")
```

— so the whole app crashes on a background tokio worker thread:

```
thread 'tokio-rt-worker' panicked at src/gadgets/clipboard/mod.rs:166:44:
clipboard context: SetupFailed(SetupFailed { .. })
```

The same failure mode applies to anything that legitimately blocks
clipboard setup: running under `sudo`, a broken display server, a
container without an X socket, a forthcoming Wayland-only session
where XWayland is absent, etc. In every one of those cases the
clipboard feature should degrade gracefully — the rest of the app has
no need of it.

## Surface area

`src-tauri/src/gadgets/clipboard/mod.rs` has several `.expect(...)`
sites that fall into two groups:

- **Mutex poisoning asserts** (lines 135, 137, 153, 199, 234, 237,
  294, 301, 310, 311, 322, 339, 434): these are correct — if a mutex
  is poisoned the gadget really cannot continue safely. Keep as-is.
- **External-system setup asserts** that are the actual problem:
  - `src-tauri/src/gadgets/clipboard/mod.rs:160` —
    `ClipboardWatcherContext::new().expect("create clipboard watcher")`
  - `src-tauri/src/gadgets/clipboard/mod.rs:166` —
    `ClipboardContext::new().expect("clipboard context")`

There is already a precedent for graceful handling elsewhere in the
same file:

- `src-tauri/src/gadgets/clipboard/mod.rs:502` — an occurrence of
  `ClipboardContext::new()` in the active-query path that uses
  `match` and falls through to an error branch. That is the pattern
  the startup path should adopt.

## What to do

1. Replace the two setup-path `.expect(...)` calls with proper error
   handling: log a warning (`eprintln!` or `tracing::warn!`) that
   names the underlying error, skip spawning the watcher + retention
   threads, and mark the gadget as disabled-for-this-session. The
   rest of the app must continue normally — including the tray menu,
   the launcher, and every other gadget.
2. The clipboard view / command-palette entries that depend on the
   gadget need to handle the disabled state (probably already handled
   by the `lc.running = true` gate at line 171 — double-check that
   no caller assumes it is always true).
3. Add an eprintln with the resolved underlying cause when possible,
   because `SetupFailed { .. }` alone has opaque `Debug` output. The
   caller's eyes are what needed debugging on Fedora — surfacing the
   X11 error string in particular would have saved the whole
   XAUTHORITY trail.

## Non-goals

- Fixing the X11-only nature of `clipboard-rs` 0.3 on Linux. That is
  a separate and larger change — see
  `todos/fedora/01kpv8y2mb5ht7sp63dgey3nhg-wayland-aware-clipboard-watcher.md`.
