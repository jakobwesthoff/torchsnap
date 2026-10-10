---
title: "Control show fails before the layout is known while the hotkey queues the show"
kind: bug
component: backend
status: needs-discussion
impact: low
---
# Control show fails before the layout is known while the hotkey queues the show

`docs/control-api.md` documents `show` as "matching the behavior of the keyboard shortcut". For the case where the launcher layout has not reached the backend yet, the two paths behave differently: the hotkey queues the show, and the control `show` returns an error. The window is real during app startup, because the layout only arrives after React mounts and calls `launcher_set_layout`, and the same happens after any future webview destroy and recreate.

## Problem

Read in the code; not confirmed at runtime.

- Hotkey path: `toggle_launcher_window` in `src-tauri/src/lib.rs` handles a missing `LauncherLayoutState` by calling `layout_state.request_show()`, which queues the show. `launcher_set_layout` in the same file fires it once the frontend reports its layout.
- Control path: `ShowHandler` in `src-tauri/src/control/handlers/launcher.rs` returns a JSON-RPC error instead:

  ```rust
  let layout = *app.state::<LauncherLayoutState>()
      .get()
      .ok_or_else(|| ControlError::Internal {
          message: "launcher layout not yet received from frontend".to_string(),
      })?;
  ```

Secondary nit: the error is classified as `Internal` (`-3`, documented as "unexpected failure in the handler") although it is a precondition, which the doc maps to `-1` invalid state.

## Impact

A script that starts Torchsnap and immediately calls `show` gets `-3` where the hotkey would have queued the show. Retry logic is pushed onto every client. The error-code taxonomy in the doc does not match what clients receive for this case.

## Suggested fix

Mirror the hotkey behavior: on a missing layout, call `request_show()` and return `{"ok": true}`. The show fires as soon as the frontend is ready. If erroring is preferred instead, return `-1` (invalid state) and document the startup precondition under the `show` method in `docs/control-api.md`.
