# Explicit gadget channel lifecycle management

## Problem

The `gadget_message` path passes a Tauri `Channel` to the backend but
has no protocol for closing it. When the frontend component unmounts,
the JS `Channel` object is GC'd and the backend only discovers this on
the next failed `send()`. This works for lazy cleanup but provides no
explicit signal, leading to channel accumulation on the backend.

This was originally identified via the clipboard gadget's channel
accumulation problem, since fixed with the single-channel
`ActiveQuery` architecture in `src-tauri/src/gadgets/clipboard/storage.rs`.

## Proposal

1. **Automatic close signaling** — when the frontend hook/wrapper tears
   down (e.g. component unmount), it sends a framework-level `"close"`
   or `"unsubscribe"` message to the backend so the gadget can clean up
   eagerly rather than waiting for a failed `send()`.
2. **Backend `handle_message` integration** — the gadget trait's
   `handle_message` (or a wrapper around it) should intercept the close
   message and handle channel teardown generically, so individual gadgets
   don't each need to implement cleanup logic.
3. **Frontend abstraction** — a hook (e.g. `useGadgetChannel`) that
   manages channel creation, message dispatch, and automatic close on
   unmount as a single unit. This replaces the current ad-hoc
   `useGadgetStream` / `sendGadgetMessage` pair.

## Open questions

- Should the close signal be a dedicated Tauri command or a reserved
  `method` string (e.g. `"__close"`) on `gadget_message`?
