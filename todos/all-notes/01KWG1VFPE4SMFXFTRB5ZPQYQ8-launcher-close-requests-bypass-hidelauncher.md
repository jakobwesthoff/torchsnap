---
title: "Launcher close requests bypass hide_launcher"
kind: bug
component: backend
impact: low
---
# Launcher close requests bypass hide_launcher

Closing the launcher window hides it with a direct `win.hide()` call instead of the regular dismiss path. That path skips the window shrink that releases WebKit backing stores (ADR 0020, memory), the Linux blanking step, and the NSPanel bookkeeping that keeps `toggle_launcher_window` in sync.

## Problem

Read in the code; not confirmed at runtime.

All regular dismiss paths go through `hide_launcher` and `request_launcher_dismiss` in `src-tauri/src/lib.rs`. `hide_launcher` shrinks the window to 1x1 so WebKit releases its backing stores (ADR 0020 memory rationale), and on Linux it runs the frontend blanking step before hiding. The `launcher_hide` command doc says it exists "ensuring the window shrink always happens".

The window-close interception in the `RunEvent::WindowEvent` handler of `src-tauri/src/lib.rs` does neither:

```rust
RunEvent::WindowEvent { label, event: WindowEvent::CloseRequested { api, .. }, .. }
    if label == "main" => {
    api.prevent_close();
    if let Some(win) = app.get_webview_window(label) {
        let _ = win.hide();
    }
}
```

It hides via `win.hide()` directly. The full-size backing store stays allocated, and on Linux the next show flashes the stale pre-hide frame, which is the problem the `launcher-dismiss-requested` event exists to prevent. It also bypasses `PlatformLauncherPanel::hide`, so on macOS the NSPanel side and the Tauri window-visibility side can disagree. `toggle_launcher_window` consults `PlatformLauncherPanel::is_visible`, so it can then treat a launcher that is hidden as visible.

`CloseRequested` for the launcher is reachable at least through Cmd+W-style accelerators or window-manager close on Linux and Windows.

## Impact

After a close-requested dismiss: elevated memory until the next proper hide, a stale-frame flash on Linux, and possibly a launcher that `toggle_launcher_window` still considers visible, so the first hotkey press dismisses instead of showing.

## Suggested fix

Call `request_launcher_dismiss(app)` in the handler instead of `win.hide()`.
