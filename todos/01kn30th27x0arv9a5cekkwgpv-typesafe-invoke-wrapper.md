# Typesafe `invoke` wrapper for Tauri commands

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

Introduce a typed wrapper (hook or plain function) that:

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

## Open questions

- Should the type registry be hand-maintained or generated from Rust
  (e.g. via `ts-rs` or `specta`)?
- Is a hook necessary, or is a typed standalone function enough?
- Error handling strategy: silent log, toast, or propagate to caller?
