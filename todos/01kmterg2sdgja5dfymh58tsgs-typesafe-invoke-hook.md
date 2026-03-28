# Typesafe `useInvoke` hook for Tauri commands

## Problem

`invoke()` from `@tauri-apps/api/core` is used directly throughout the
frontend with string command names and untyped parameters/return values.
This is error-prone: typos in command names, wrong parameter shapes, and
missing error handling are all silent until runtime.

Current call sites include:
- `useWindowLifecycle.ts` — `invoke("launcher_hide")`
- `useSearch.ts` / `useControlChannel.ts` — `invoke("search_query", ...)`,
  `invoke("search_execute", ...)`
- `pluginMessage.ts` — `invoke("plugin_message", ...)`
- `Launcher.tsx` — `invoke("search_execute", ...)`

## Proposal

Introduce a `useInvoke` hook (or a typed wrapper module) that:

1. Defines a command registry mapping command names to their
   parameter and return types (derived from the Rust `#[tauri::command]`
   signatures).
2. Returns a typesafe async function where the command name is
   constrained to known commands and params/return are inferred.
3. Centralizes error handling (e.g. logging, user-facing errors).

```ts
// Sketch — exact API TBD
const invoke = useInvoke();
await invoke("launcher_hide"); // no params, no return — OK
await invoke("search_query", { query: "foo" }); // typed params
const result = await invoke("search_execute", { ... }); // typed return
```

Alternatively a plain function (no hook) with a type map may suffice if
no React context is needed.

## Channel lifecycle management

The `plugin_message` path currently passes a Tauri `Channel` to the
backend but has no protocol for closing it. When the frontend component
unmounts, the JS `Channel` object is GC'd and the backend only discovers
this on the next failed `send()`. This works for lazy cleanup but has no
explicit signal.

As part of this refactor, the typed abstraction should include:

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

This is directly motivated by the clipboard plugin's channel
accumulation problem (see `01kmtn6msqqjpnc9aeb3rh8gnw`), but applies to
any future plugin that holds persistent channels.

## Open questions

- Should the type registry be hand-maintained or generated from Rust
  (e.g. via `ts-rs` or `specta`)?
- Is a hook necessary, or is a typed standalone function enough?
- Error handling strategy: silent log, toast, or propagate to caller?
- Should the close signal be a dedicated Tauri command or a reserved
  `method` string (e.g. `"__close"`) on `plugin_message`?
