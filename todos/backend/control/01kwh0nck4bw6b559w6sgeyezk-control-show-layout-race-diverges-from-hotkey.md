---
kind: bug
severity: low
status: open
area: [src-tauri/src/control/handlers/launcher.rs]
tags: [unconfirmed]
---

# Control `show` errors before layout is reported; hotkey path queues instead

## Problem

`docs/control-api.md:81-82` documents `show` as "matching the
behavior of the keyboard shortcut". The layout-not-ready edge
case behaves differently between the two paths:

- Hotkey path: `toggle_launcher_window`
  (`src-tauri/src/lib.rs:486-489`) handles a missing
  `LauncherLayoutState` by calling `layout_state.request_show()`,
  which queues the show; `launcher_set_layout`
  (`lib.rs:468-473`) fires it once the frontend reports its
  layout.
- Control path: `ShowHandler`
  (`src-tauri/src/control/handlers/launcher.rs:56-62`) returns a
  JSON-RPC error instead:

  ```rust
  let layout = *app.state::<LauncherLayoutState>()
      .get()
      .ok_or_else(|| ControlError::Internal {
          message: "launcher layout not yet received from frontend".to_string(),
      })?;
  ```

The window is real during app startup (layout arrives only after
React mounts and calls `launcher_set_layout`) and would recur for
any future webview destroy/recreate cycle.

Secondary nit: the error is classified as `Internal` (`-3`,
documented as "unexpected failure in the handler",
`docs/control-api.md:203`) although it is a precondition, which
the doc maps to `-1` invalid state (`control-api.md:202`).

## Impact

A script that starts Torchsnap and immediately calls `show` gets
`-3` where the hotkey would have queued the show; retry logic is
pushed onto every client. Error-code taxonomy in the doc does not
match what clients actually receive for this case.

## Suggested fix

Mirror the hotkey behavior: on missing layout call
`request_show()` and return `{"ok": true}` (the show fires as
soon as the frontend is ready). If erroring is preferred instead,
return `-1` (invalid state) and document the startup precondition
under the `show` method in `docs/control-api.md`.
