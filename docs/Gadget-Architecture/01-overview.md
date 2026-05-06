# Gadget Architecture Overview

Torchsnap gadgets are sandboxed WebAssembly components conforming to the
`torchsnap:gadget@0.1.0` world defined in
`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`. The host (Rust + Tauri)
loads them at runtime, mediates every capability they reach for, and
streams their search results into the launcher UI.

A small number of capabilities (clipboard, app launcher, system
preferences, system commands) still ship as **Builtin** native Rust
gadgets compiled into the host binary. Everything new is WASM. Both
kinds implement the same `Plugin` trait
(`src-tauri/src/gadgets/mod.rs`) — for WASM gadgets that trait is
implemented by `WasmGadgetBridge` (`src-tauri/src/wasm/bridge.rs`),
which forwards every call across the WIT boundary.

## Component model

- **Target:** `wasm32-wasip2`. The `gadgets/` directory is a virtual
  workspace whose default target is set in `gadgets/.cargo/config.toml`.
- **No `cargo-component`.** Gadgets build with plain
  `cargo build --release`. The single `wit_bindgen::generate!` call
  lives in the `torchsnap-gadget-sdk` crate
  (`gadgets/gadget-sdk/src/lib.rs`); gadget crates pull the generated
  bindings through `use torchsnap_gadget_sdk::prelude::*;` and register
  their type with `define_gadget!(MyGadget)`.
- **Engine:** `wasmtime` with the component model. A single
  `WasmRuntime` (`src-tauri/src/wasm/runtime/engine.rs`) owns the
  shared `Engine` and a per-gadget `Component` cache.

## The `gadget` world

A gadget imports host capabilities and exports the four guest
interfaces the host calls into. The full set, from
`torchsnap-gadget.wit`:

```
world gadget {
  import logging;
  import clipboard;
  import sql;
  import frecency;
  import settings;
  import opener;
  import http;
  import fs;
  import assets;
  import command;
  import platform;
  import paths;
  import types;
  import website-metadata;

  export lifecycle;   // enable / disable / on-setting-changed
  export search;      // entries / search / execute
  export messaging;   // handle-message (frontend RPC)
  export tasks;       // run-task (cron-scheduled)
}
```

Gadgets that don't use messaging or scheduled tasks emit no-op
implementations via `impl_noop_messaging!` / `impl_noop_tasks!` from
the SDK — both exports are world-mandated.

## Distribution

Gadgets ship as `.torchsnap` zip archives, or live as plain
directories during development. Discovery
(`src-tauri/src/wasm/discovery.rs`) walks three precedence-ordered
roots:

1. **System** — `<resource_dir>/gadgets/`. Populated from
   `target/bundled-gadgets/` by the `stage-bundled-gadgets` Just
   recipe at build time. Only gadgets whitelisted in
   `gadgets/bundled.toml` end up here. Not uninstallable at runtime.
2. **Dev** — `<CARGO_MANIFEST_DIR>/../gadgets/` in debug builds only,
   stripped from release artifacts via `cfg(debug_assertions)`. Tagged
   `GadgetSourceKind::Dev` for the settings UI badge.
3. **User** — `<app_data_dir>/gadgets/`. Install target for
   user-dropped `.torchsnap` archives. Uninstallable.

Within a single root, an archive shadows a sibling directory of the
same stem. Across roots, the earlier root wins and the collision is
logged. Source kinds (`Builtin`, `System`, `User`, `Dev`) are tracked
on `GadgetSourceKind` (`src-tauri/src/wasm/source.rs:52`) and
serialized to the frontend for badging and uninstall gating. See
ADR 0035 for the full distribution flow and ADR 0036 for the trust
model.

The `GadgetSource` trait abstracts directory vs. archive reads.
`DirectorySource` and `ArchiveSource` are the two implementations; the
rest of the host treats them interchangeably.

## Gadget layout

A gadget (whether archived or a directory) contains:

- `manifest.toml` — typed by `wasm::manifest::Manifest`. Declares
  `[gadget]` metadata, `[settings]` defaults, `[shortcuts]`,
  `[frontend]`, `[storage.sql]` migrations, `[[tasks]]`, and the
  per-capability `[permissions.*]` blocks.
- The compiled WASM component referenced by `[gadget] wasm = "..."`.
- Optional frontend bundles (referenced from `[frontend]`).
- Optional bundled assets readable through the `assets` host
  interface.

## Storage layout

Code and host-managed state are split:

- **Code:** `<app_data_dir>/gadgets/<gadget-id>/` (User-installed) or
  `<resource_dir>/gadgets/<gadget-id>/` (System).
- **Host-managed state:** `<app_data_dir>/gadget-home/<gadget-id>/`.
  Contains the SQLite database at `sql/storage.sqlite3` and reserves
  sibling slots for future blob / cache / files storage.

The `${gadget-data}` substitution variable resolves to the gadget's
`gadget-home` directory; `${gadget-archive}` resolves to the gadget's
code root. See ADR 0018 (SQL storage) and ADR 0035 for the layout
contract.

## Lifecycle (ADR 0033)

The host splits load and run into two phases so disabled gadgets cost
only a cached `Component`, not a live store.

1. **Compile at load.** `WasmGadgetBridge::new`
   (`src-tauri/src/wasm/bridge.rs`) reads the manifest, reads the
   gadget's WASM bytes via the `GadgetSource`, and calls
   `WasmRuntime::compile`. Broken components fail fast at startup
   rather than on first enable.
2. **Instantiate on enable.** When the gadget is enabled — at startup
   if its `enabled.<gadget-id>` setting is true, or later when the
   user toggles it on — the bridge calls `WasmRuntime::instantiate`
   to build a fresh `WasmGadgetInstance`
   (`src-tauri/src/wasm/runtime/instance.rs`) with its own wasmtime
   `Store`, materializes the SQL database (creating files and running
   `[storage.sql]` migrations the first time), wires up host
   capability state on the instance from manifest permissions, then
   invokes the guest's `lifecycle::enable`.
3. **Disable** drops the instance, reclaiming the store and all
   guest linear memory. The cached `Component` stays so a later
   re-enable doesn't re-compile.
4. **Setting changes** flow through a host-side
   `CoalescingDispatcher` (ADR 0026) that deduplicates rapid writes
   to the same key, then call `lifecycle::on-setting-changed(key,
   json)` on the live instance. `key` is namespace-relative (the
   `gadgets.<id>.` prefix is stripped).
5. **Execute** is invoked when the user activates a result; it
   returns a `post-action` (`nothing` / `dismiss` / `keep-open`)
   plus, on the host trait surface, an optional `ShowCustomUI`
   variant.
6. **Scheduled tasks.** If the manifest declares `[[tasks]]`, the
   bridge spawns a tokio scheduler loop that walks every parsed cron
   expression and invokes `tasks::run-task(task-id)` on the live
   instance. The loop is co-operatively shut down via an
   `AtomicBool` flag on disable.

## Enable / disable gating

The host owns the enabled state (ADR 0025). Each gadget is wrapped in
a `GadgetSlot` that holds an `AtomicBool` for the enabled flag plus
the `CoalescingDispatcher` for `on-setting-changed`. The enabled key
lives at `enabled.<gadget-id>` in the top-level settings namespace —
outside the `gadgets.<id>.*` prefix and unreachable from the gadget
itself. The host intercepts changes to that key and drives
`enable()` / `disable()` directly.

## Search dispatch

Three modes coexist (ADR 0023, ADR 0024):

- **Catalog.** The gadget returns a finite list of `catalog-entry`
  values from `search::entries`. The host runs nucleo fuzzy matching
  against `title + keywords`. Suitable for app launchers, system
  preferences, clipboard history.
- **Query.** Every keystroke calls `search::search(query,
  matched-prefix)`. The gadget scores its own results and returns a
  `search-response`.
- **Prefix-routed query.** A query gadget can declare exclusive
  prefixes (e.g. `=` for the calculator, `!` for bangs). When the
  query starts with one, only that gadget's `search` runs and the
  matched prefix is forwarded so the gadget sees the original
  trigger character (ADR 0012).

Results stream over a Tauri channel as `SearchMessage`s
(`src-tauri/src/search/types.rs`). The frontend merges per-source
batches into a single sorted list using the same `cmp_sort_key`
ordering as the Rust side. Catalog and query results share the
`SearchResults` variant; `Done` terminates the stream. The two-tier
display (custom-ui replacing the result list, inline-ui above it,
both gated to a single gadget per search) is ADR 0008 + ADR 0021.

## Frontend integration

Gadgets ship optional React frontends declared under `[frontend]` in
the manifest. Custom and inline views are looked up by name from the
gadget's component registry (ADR 0022 / ADR 0028). The frontend can
make synchronous RPC calls back into the gadget via Tauri's invoke
boundary, which routes to the WIT `messaging::handle-message` guest
export — request/response only, no streaming, JSON-encoded payloads
on both sides.

## Host capabilities

Every host import is gated by the manifest. A capability the
manifest doesn't grant is unreachable; the host returns
`permission-denied(...)` (or the interface's equivalent variant) at
call time.

| Interface | Manifest gate | Notes |
|---|---|---|
| `logging` | always | Structured logs + timing spans, routed to host logger. |
| `clipboard` | always | Write-only (`write-text`). Read intentionally not exposed. |
| `settings` | always | Scoped to `gadgets.<id>.*`; JSON-encoded values. |
| `frecency` | always | Read-only top-N by score; tracking happens automatically host-side. |
| `assets` | always | Spatial guarantee: paths validated to stay inside the gadget root. |
| `platform` | always | OS / arch detection. |
| `paths` | always | `${...}` substitution against host-resolved paths. |
| `sql` | `[storage.sql]` declared | Per-gadget SQLite at `gadget-home/<id>/sql/storage.sqlite3`. |
| `opener` | `[permissions.opener]` | Per-capability flags: `schemes`, `open-path`, `reveal-path`. |
| `http` | `[permissions.http] origins` | Origin allowlist or `"*"` for trust-all. |
| `fs` | `[permissions.fs] read` | Globs canonicalized; symlinks resolved before match. |
| `command` | `[[permissions.command]]` | Per-binary argv-shape rules; no shell wrapping. |
| `website-metadata` | `[permissions] website-metadata = true` | Host-shared cache; favicons returned as `entry-icon`. |

The `${gadget-data}`, `${gadget-archive}`, `${home}`,
`${xdg-config}`, `${xdg-data}` substitution variables are recognized
in `[permissions.command]` `path-under` roots, literal argv values,
per-rule `cwd`, and `[permissions.fs] read` patterns. See the
relevant ADR (0029–0032, 0037–0040) for each capability's design
rationale.

## SDK ergonomics

`torchsnap-gadget-sdk` (`gadgets/gadget-sdk/src/`) re-exports the
generated bindings and adds higher-level helpers on top of the raw
WIT interfaces:

- `prelude::*` brings the four guest traits, the search records, the
  flat host import aliases, and the `define_gadget!` /
  `impl_noop_*!` macros into scope.
- `settings::get` / `get_or` / `get_or_else` fold the JSON parse
  into the lookup.
- `sql::Row` plus `query_one` / `query_all` give typed column access
  over the raw `Vec<Vec<sql-value>>` interface.
- `command`, `messaging`, `logging`, `website_metadata` modules wrap
  the raw imports with conveniences (typed builders, `tracing`-style
  span helpers, etc.).

A minimal gadget is roughly:

```rust
use torchsnap_gadget_sdk::prelude::*;

struct MyGadget;
torchsnap_gadget_sdk::define_gadget!(MyGadget);
impl_noop_messaging!(MyGadget);
impl_noop_tasks!(MyGadget);

impl LifecycleGuest for MyGadget { /* enable / disable / on_setting_changed */ }
impl SearchGuest for MyGadget { /* entries / search / execute */ }
```
