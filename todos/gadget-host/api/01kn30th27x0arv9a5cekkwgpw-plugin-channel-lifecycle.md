# Explicit plugin channel lifecycle management

## Problem

The `plugin_message` path passes a Tauri `Channel` to the backend but
has no protocol for closing it. When the frontend component unmounts,
the JS `Channel` object is GC'd and the backend only discovers this on
the next failed `send()`. This works for lazy cleanup but provides no
explicit signal, leading to channel accumulation on the backend.

This was originally identified via the clipboard plugin's channel
accumulation problem (see `01kmtn6msqqjpnc9aeb3rh8gnw`).

## Proposal

1. **Automatic close signaling** — when the frontend hook/wrapper tears
   down (e.g. component unmount), it sends a framework-level `"close"`
   or `"unsubscribe"` message to the backend so the plugin can clean up
   eagerly rather than waiting for a failed `send()`.
2. **Backend `handle_message` integration** — the plugin trait's
   `handle_message` (or a wrapper around it) should intercept the close
   message and handle channel teardown generically, so individual plugins
   don't each need to implement cleanup logic.
3. **Frontend abstraction** — a hook (e.g. `usePluginChannel`) that
   manages channel creation, message dispatch, and automatic close on
   unmount as a single unit. This replaces the current ad-hoc
   `usePluginStream` / `sendPluginMessage` pair.

## Open questions

- Should the close signal be a dedicated Tauri command or a reserved
  `method` string (e.g. `"__close"`) on `plugin_message`?
