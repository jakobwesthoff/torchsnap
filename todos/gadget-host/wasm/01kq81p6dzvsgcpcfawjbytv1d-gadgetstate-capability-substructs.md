# Refactor `GadgetState` flat fields into capability sub-structs

## Context

`src-tauri/src/wasm/runtime.rs::GadgetState` accumulates one set of
fields per host capability. The flat pattern was fine while each
capability was 1–2 fields. With the command + opener-extension work
(ADR 0040), the field count crosses the threshold where cohesion
matters:

- `opener` now owns 6 fields: `opener_schemes`, `opener_open_path`,
  `opener_reveal_path`, `opener_writer`, `open_path_writer`,
  `reveal_path_writer`.
- `command` (still in flight) will add `command_rules` and
  `running_child`.
- `http` is still 2 (`http_origins`, `http_client`).
- `sql` is split between `sql_config`, `sql_storage`,
  `sql_handle_reps`.

The result is ~30 weakly-related top-level fields plus a parallel set
of setters/clearers on `WasmGadgetInstance`, all of which the bridge's
`enable()` / `disable()` paths walk individually.

## Target

Replace the flat fields with one struct per capability. Each
capability struct owns its bridge-stashed state and exposes an
`enable(...)` constructor + `disable(self)` cleanup (or `Default` +
mutating `enable_*`). The bridge's enable path becomes a sequence of
single-field assignments per capability instead of N setter calls.

Sketch (not committing to exact shape):

```rust
struct GadgetState {
    wasi: WasiCtx,
    wasi_table: ResourceTable,
    settings: Option<GadgetSettings>,
    frecency: Option<GadgetFrecency>,
    sql: SqlState,
    clipboard: ClipboardState,
    opener: OpenerState,
    http: HttpState,
    command: CommandState,
    paths: Option<PathContext>,
    gadget_source: Option<Arc<dyn GadgetSource + Send + Sync>>,
}
```

Existing precedent: the host already groups SQL bridge state into
`SqlConfig`. The pattern works.

## Why deferred

This is a structural refactor that touches ~30 fields across
`GadgetState`, `WasmGadgetInstance` setter/clearer methods, the
bridge's `enable`/`disable`, and the unit tests that drive the
fixture gadgets. Doing it together with the ADR 0040 implementation
phases would mix capability work and structural rework in the same
commit and make the diff hard to review. Better as a focused
follow-up commit once Phase E lands and the full set of fields the
refactor needs to touch is visible.

## Non-goals

- Changing the WIT surface or any externally-observable contract.
  This is a host-internal refactor.
- Reworking the `enable()`/`disable()` ordering. The capability
  initialization order matters (e.g. SQL must be ready before guest
  enable so `sql::connection()` works); the refactor preserves it.
- Refactoring the `WasmGadgetInstance` setter/clearer surface in any
  way that would break the existing test fixtures' setup helpers
  in one go — those tests can be migrated incrementally.

## Blocked-by / enables

- Depends on: ADR 0040 Phase E (host runtime impls) landing first so
  the full set of fields is in place.
- Enables: cleaner future capability additions; smaller surface for
  the install-consent and settings-UI todos to introspect.
