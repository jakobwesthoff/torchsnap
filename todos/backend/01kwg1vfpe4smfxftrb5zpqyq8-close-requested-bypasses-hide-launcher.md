---
kind: bug
severity: low
status: open
area: [src-tauri/src/lib.rs]
tags: [unconfirmed]
---

# Launcher CloseRequested handler bypasses hide_launcher (no shrink, no Linux blanking)

## Problem
All regular dismiss paths funnel through
`hide_launcher`/`request_launcher_dismiss`, which (a) shrink the
window to 1×1 so WebKit releases its backing stores
(`src-tauri/src/lib.rs:355-371`, ADR 0020 memory rationale) and
(b) on Linux run the frontend blanking dance before hiding
(`lib.rs:373-411`). The `launcher_hide` command doc explicitly
says it exists "ensuring the window shrink always happens"
(`lib.rs:413-419`).

The window-close interception does neither
(`lib.rs:983-993`):

```rust
RunEvent::WindowEvent { label, event: WindowEvent::CloseRequested { api, .. }, .. }
    if label == "main" => {
    api.prevent_close();
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.hide();
    }
}
```

It hides via `win.hide()` directly — the full-size backing
store stays allocated, and on Linux the next show will flash
the stale pre-hide frame (the exact problem the
`launcher-dismiss-requested` event exists to prevent). It also
bypasses `PlatformLauncherPanel::hide`, so on macOS the NSPanel
side and the Tauri window-visibility side can disagree about
visibility state (`toggle_launcher_window` consults
`PlatformLauncherPanel::is_visible`, `lib.rs:477`).

`CloseRequested` for the launcher is reachable at least via
Cmd+W-style accelerators or window-manager close on
Linux/Windows.

## Impact
After a close-requested dismiss: elevated memory until the next
proper hide, a stale-frame flash on Linux, and potentially a
launcher that `toggle_launcher_window` thinks is still visible
(first hotkey press dismisses instead of shows).

## Suggested fix
Call `request_launcher_dismiss(app)` in the handler instead of
`win.hide()`.
