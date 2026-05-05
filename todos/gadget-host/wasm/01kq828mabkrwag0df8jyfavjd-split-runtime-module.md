# Split `src-tauri/src/wasm/runtime.rs` into focused sub-modules

## Context

`src-tauri/src/wasm/runtime.rs` has grown to ~3200 lines after the
ADR 0040 host-runtime work. It now mixes:

- The `WasmRuntime` engine (component compile / instantiate cache).
- The `GadgetState` struct (the wasmtime store data carrier).
- Host trait impls for **every** host import in the WIT —
  `logging`, `settings`, `frecency`, `clipboard`, `sql`, `opener`,
  `http`, `assets`, `command`, `platform`, `paths`.
- The `WasmGadgetInstance` lifecycle and its full setter / clearer
  surface for every capability.
- The `command::run` runtime: env sanitation, process group
  termination, capped output drain, audit logging.
- A 1000-line test module with fixture-gadget compile helpers.

The file is hard to navigate, hard to review when only one
capability is touched, and hard to keep under intellectual control as
new capabilities arrive.

## Target

Move each concern into a focused sub-module under
`src-tauri/src/wasm/runtime/`:

```
src-tauri/src/wasm/runtime/
├── mod.rs              — re-exports, doc comments, the
│                         module-level "what lives here" map
├── engine.rs           — `WasmRuntime`: compile, instantiate,
│                         component cache, span registry handle
├── state.rs            — `GadgetState` struct + `Default` impl
├── instance.rs         — `WasmGadgetInstance`: lifecycle methods,
│                         setters/clearers, guest call shims
│                         (`enable`, `disable`, `entries`, `search`,
│                         `execute`, `handle_message`, `run_task`,
│                         `on_setting_changed`)
├── host_logging.rs     — `logging::Host` impl + span registry glue
├── host_settings.rs    — `settings::Host`
├── host_frecency.rs    — `frecency::Host`
├── host_clipboard.rs   — `clipboard::Host`
├── host_sql.rs         — `sql::Host`, `SqlConfig`, `SqlHandleEntry`
├── host_opener.rs      — `opener::Host`, `check_opener_scheme`,
│                         `OpenerSchemeCheckError`,
│                         `UrlOpenerFn` type alias
├── host_http.rs        — `http::Host`, `check_http_origin`,
│                         method/header conversion helpers
├── host_assets.rs      — `assets::Host`
├── host_platform.rs    — `platform::Host`
├── host_paths.rs       — `paths::Host`
├── host_command.rs     — `command::Host`, `run_child_with_caps`,
│                         `kill_child_with_grace`,
│                         `build_command_env`, env denylist consts,
│                         `read_capped`, `signal_name`,
│                         `emit_command_audit`
└── tests.rs            — the existing `#[cfg(test)] mod tests`
                          (or split per-capability test files later)
```

This is a touch-everything-once refactor. No behavior changes — only
file reorganization, visibility tweaks (some currently-private items
become `pub(super)` to cross module boundaries), and `mod.rs`
re-exports so the rest of the crate keeps importing things from
`super::runtime::...`.

## Why deferred

- The Phase H refactor (`GadgetState` → capability sub-structs)
  shrinks each Host impl considerably and may change the natural
  grouping (e.g. `OpenerState`/`HttpState`/`CommandState` could
  each live alongside their Host impl). Doing the file split *first*
  would require revisiting every grouping after Phase H lands.
- Pure mechanical split commits land cleaner once feature work is
  done; mid-feature splits invite merge headaches.

The sensible ordering is:

1. Land Phases F (SDK) and G (test fixture + integration tests) so
   the runtime impls have their consumers.
2. Land Phase H (capability sub-structs) so each Host impl owns less
   state and the grouping by capability is the natural shape.
3. Then this split — file boundaries follow capability boundaries,
   no further refactor needed.

## Non-goals

- Behavior change. This is a structural refactor only. No
  performance work, no API change, no new features.
- Splitting the test fixture gadgets. The existing
  `src-tauri/tests/fixtures/*-gadget/` crates stay as-is.
- Reworking how host imports are added to the linker. The bindgen-
  emitted `Gadget::add_to_linker` already finds Host impls
  regardless of which module they live in.

## Blocked-by / enables

- Depends on: ADR 0040 Phase F, G, H completing first (see above).
- Pairs with the Phase H refactor todo
  (`todos/wasm/01kq81p6dzvsgcpcfawjbytv1d-...md`); doing them in the
  recommended order means each capability's state struct and Host
  impl land together in the same source file.
- Enables: faster code review on capability-scoped changes;
  capability-scoped per-file unit tests; lower bar for adding new
  WIT host imports (one new file vs. another append to the
  monolith).
