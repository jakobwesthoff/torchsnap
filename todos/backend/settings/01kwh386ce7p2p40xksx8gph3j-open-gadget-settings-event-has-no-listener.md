# `OpenSettings` action emits `open-gadget-settings` into the void — no listener exists

**Kind:** bug
**Severity:** high
**Area:** src-tauri/src/gadget_host.rs, src/settings/

## Problem

When a user executes an entry whose action is
`ActionId::OpenSettings`, `GadgetHost::execute` short-circuits
and emits an event (`src-tauri/src/gadget_host.rs:969-977`):

```rust
if matches!(action_id, ActionId::OpenSettings) {
    if let Err(e) = app.emit(
        "open-gadget-settings",
        serde_json::json!({ "gadgetId": source }),
    ) { ... }
    return Ok(PostAction::Dismiss);
}
```

Nothing listens for `"open-gadget-settings"`. Verified
2026-07-02 by searching the whole repo: the only code hit is
the emit site itself; there is no `listen` registration in any
frontend window (`src/`, `packages/`) nor in Rust. Two
independent gaps compound:

1. **The settings window is not even opened.** Unlike the
   `PostAction::ShowSettings` path directly below
   (`gadget_host.rs:996-998`), which calls
   `crate::show_settings_window(app)`, the `OpenSettings` path
   only emits. Tauri events are not queued for windows that do
   not exist yet, so with the settings window closed (the
   common case when using the launcher) the event is dropped
   outright.
2. **Even with the settings window open, nothing navigates.**
   The settings frontend has no listener that would switch the
   panel to the given `gadgetId`.

The net user-visible behavior of any `OpenSettings` action is:
launcher dismisses, nothing else happens.

## Impact

This is a live user-facing path, not dead code: the zerotier
gadget uses `ActionId::OpenSettings` as the primary action on
its synthetic "configuration required" entries
(`gadgets/zerotier/src/lib.rs:322,327`). A user who searches,
sees "ZeroTier needs configuration", and presses Enter gets a
dismissed launcher and no settings window — the exact
remediation the entry exists to offer is unreachable through
it.

The architecture doc also promises the working behavior
(`docs/Gadget-Architecture/06-settings-reactivity.md:351-354`:
"…so the settings window can jump straight to that gadget's
panel"), so the doc is currently describing fiction.

## Suggested fix

In the `OpenSettings` intercept:

1. Call `crate::show_settings_window(app)` first (same as the
   `PostAction::ShowSettings` arm) so the window exists.
2. Deliver the target gadget id in a way that survives window
   creation — either emit after the window's `react-ready`
   handshake (see `src-tauri/src/lib.rs:253`), or persist the
   pending target (managed state read by a settings-window
   command on mount) instead of a fire-and-forget event.
3. Add the frontend listener/navigation in the settings window
   that selects `gadgetId`'s panel.

Related (does not cover this): the pre-existing todo
`todos/01krp5a92mezsrq8jg14mm3nqx-opensettings-passthrough.md`
proposes rearchitecting the interception into a gadget-side
`PostAction`; whichever design wins, the
window-open + navigation gap fixed here must be part of it.
