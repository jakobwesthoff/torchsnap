# Gadget Architecture Overview

Torchsnap gadgets are sandboxed WebAssembly components conforming to the
`torchsnap:gadget@0.2.0` world defined in
`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`. The host (Rust + Tauri)
loads them at runtime, mediates every capability they reach for, and
streams their search results into the launcher UI.

The WIT package, the Rust SDK crate `torchsnap-gadget-sdk` and the
TypeScript SDK package `@torchsnap/gadget-sdk` share one version
number, currently 0.2.0. When any of the three changes incompatibly,
all three move to the same new version (ADR 0056).

A small number of capabilities (clipboard, app launcher, system
preferences, system commands) still ship as **Builtin** native Rust
gadgets compiled into the host binary. Everything new is WASM. Both
kinds implement the same `Gadget` trait
(`src-tauri/src/gadgets/mod.rs`), whose supertrait `ErasedSearch`
holds `entries()`, `search()` and `execute()` with action commands
as opaque strings. Native gadgets implement `Gadget` plus the typed
`Search` trait, and a blanket impl turns `Search` into
`ErasedSearch`. For WASM gadgets, both traits are implemented by
`WasmGadgetBridge` (`src-tauri/src/wasm/bridge.rs`), which forwards
every call across the WIT boundary and passes command strings
through unchanged.

## Component model

- **Target:** `wasm32-wasip2`. The `gadgets/` directory is a virtual
  workspace whose default target is set in `gadgets/.cargo/config.toml`.
- **No `cargo-component`.** Gadgets build with plain
  `cargo build --release`. The single `wit_bindgen::generate!` call
  lives in the `torchsnap-gadget-sdk` crate
  (`gadgets/gadget-sdk/src/lib.rs`); gadget crates pull the generated
  bindings through `use torchsnap_gadget_sdk::prelude::*;` and register
  their type with `define_gadget!(MyGadget)`.
- **Engine:** `wasmtime` with the component model, using the
  **Winch** baseline compiler (ADR 0043) for reduced memory footprint
  (~110 MB idle RSS vs ~242 MB with Cranelift for 7 gadgets). A
  single `WasmRuntime` (`src-tauri/src/wasm/runtime/engine.rs`) is a
  **stateless** compile/instantiate service that owns the shared
  `Engine`. Per-gadget `CachedComponent`
  (`src-tauri/src/wasm/runtime/cached_component.rs`) wraps compilation
  with a **disk-backed cache** (ADR 0044): compiled artifacts are
  serialized to disk and re-loaded via `deserialize_file`
  (mmap-backed, OS-pageable), reducing idle RSS further to ~52 MB
  Winch+cache.

## The `gadget` world

A gadget imports host capabilities and exports the four guest
interfaces the host calls into. The full set, from
`torchsnap-gadget.wit`:

```
world gadget {
  import logging;
  import clipboard;
  import sql-storage;
  import frecency;
  import settings;
  import opener;
  import http;
  import filesystem;
  import assets;
  import command;
  import platform;
  import path-resolver;
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
the SDK, since both exports are world-mandated.

## Distribution

Gadgets ship as `.torchsnap` zip archives, or live as plain
directories during development. Discovery
(`src-tauri/src/wasm/discovery.rs`) walks three precedence-ordered
roots:

1. **System** (`<resource_dir>/gadgets/`). Populated from
   `target/bundled-gadgets/` by the `stage-bundled-gadgets` Just
   recipe at build time. Only gadgets whitelisted in
   `gadgets/bundled.toml` end up here. Not uninstallable at runtime.
2. **Dev** (`<CARGO_MANIFEST_DIR>/../gadgets/`, debug builds only).
   Stripped from release artifacts via `cfg(debug_assertions)`. Tagged
   `GadgetSourceKind::Dev` for the settings UI badge.
3. **User** (`<app_data_dir>/gadgets/`). Install target for
   `.torchsnap` archives the user installs. Uninstallable.

Within a single root, an archive shadows a sibling directory of the
same stem. Across roots, the earlier root wins and the collision is
logged. Source kinds (`Builtin`, `System`, `User`, `Dev`) are tracked
on `GadgetSourceKind` and serialized to the frontend for badging and
uninstall gating. See ADR 0035 for the full distribution flow and
ADR 0036 for the trust model.

The `GadgetSource` trait abstracts directory vs. archive reads.
`DirectorySource` and `ArchiveSource` are the two implementations; the
rest of the host treats them interchangeably.

### Install and uninstall

`src-tauri/src/gadget_install/` handles installing, replacing,
undoing and uninstalling user gadgets at runtime (ADR 0051). Each
submodule's header comment has the details.

**Entry points.** The settings file picker and drop zone
(`install_queue_submit`), files macOS asks the app to open
(`RunEvent::Opened`), and `.torchsnap` paths on the command line,
including those a second launch hands to the running instance through
`tauri-plugin-single-instance`. `intake.rs` turns each into an
absolute archive path.

**Queue** (`queue.rs`). Every path becomes a request in one
`InstallQueue`. The queue is created before the Tauri app is built and
buffers requests until `setup` starts it. A request is `Staging`,
`Ready` with a review, or `Failed`. `install-queue-changed` announces
every change; the settings window pulls `install_queue_snapshot` and
shows the first request.

**Staging** (`staging.rs`). Each archive is copied to
`<app_cache_dir>/install-staging/<ulid>.torchsnap` (16 MiB cap) and
opened there with `ArchiveSource::open`, which validates the zip,
parses the manifest and runs the path guard. The review and the
install both use the staged copy.

**Review** (`review.rs`, `provenance.rs`). Lists every grant of the
manifest as its own item with a severity, and on a replace marks what
is new and what the new version drops. On macOS it adds the download
origin from the `kMDItemWhereFroms` and quarantine extended
attributes. The frontend groups the items by what they touch
(`src/settings/install/permissionModel.ts`).

**Decision** (`decision.rs`). Pure functions over two views of an id:
the registry snapshot taken at startup (`registered.rs`, frozen like
the host's slot list) and the changes made since (`pending.rs`).
Built-in, bundled and dev ids are rejected; an installed user archive
is replaced, unless it is a directory form or the incoming manifest
declares fewer SQL migrations. Confirming re-runs the decision under
the queue and pending-changes locks.

**Filesystem steps** (`archive_ops.rs`). A fresh install publishes the
staged copy as `gadgets/<id>.torchsnap`. The first replace of an id in
a session keeps the current archive as `gadgets/.<id>.torchsnap.prev`
so undo can restore it. Uninstall moves the startup archive to that
backup, removes anything installed since, and writes
`gadgets/.<id>.uninstall`. At the next startup, before settings are
initialized and before any gadget loads, each marker's
`gadget-home/<id>/` tree and `enabled.<id>` / `gadgets.<id>.*`
settings keys are deleted, and leftover backups and staged copies are
removed.

**Pending changes.** `pending_gadget_changes` reports every change
since startup, which the Gadgets list shows with an Undo
(`install_undo`) and a restart bar. `restart_to_apply_gadget_changes`
writes `<app_data_dir>/.reopen-gadget-settings` and restarts; the next
setup consumes it and opens Settings on the Gadgets section.

Every change takes effect only after a restart because the
`GadgetHost` slot list is frozen after app setup. Hot
reload/lifecycle is not yet implemented.

## Gadget layout

A gadget (whether archived or a directory) contains:

- `manifest.toml`, typed by `wasm::manifest::Manifest`. Declares
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

## Lifecycle (ADR 0033, ADR 0044)

The host splits load and run into two phases so disabled gadgets cost
only a disk-cached compiled artifact, not a live store.

1. **Compile at load.** `WasmGadgetBridge::new`
   (`src-tauri/src/wasm/bridge.rs`) reads the manifest, reads the
   gadget's WASM bytes via the `GadgetSource`, and creates a
   `CachedComponent` that compiles the bytes, serializes the compiled
   artifact to disk at
   `<app_data_dir>/gadget-home/<gadget-id>/compile-cache/<blake3>.<engine_hash>.cwasm`,
   then drops the heap allocation and re-loads via `deserialize_file`
   (mmap-backed). Broken components fail fast at startup rather than
   on first enable.
2. **Instantiate on enable.** When the gadget is enabled (at startup
   if its `enabled.<gadget-id>` setting is true, or later when the
   user toggles it on), the bridge calls `WasmRuntime::instantiate`
   to build a fresh `WasmGadgetInstance`
   (`src-tauri/src/wasm/runtime/instance.rs`) with its own wasmtime
   `Store`, materializes the SQL database (creating files and running
   `[storage.sql]` migrations the first time), wires up host
   capability state on the instance from manifest permissions, then
   invokes the guest's `lifecycle::enable`.
3. **Disable** drops the instance, reclaiming the store and all
   guest linear memory. The `CachedComponent`'s disk-backed mmap stays
   so a later re-enable doesn't re-compile.
4. **Setting changes** flow through a host-side
   `CoalescingDispatcher` (ADR 0026) that deduplicates rapid writes
   to the same key, then call `lifecycle::on-setting-changed(key,
   json)` on the live instance. `key` is namespace-relative (the
   `gadgets.<id>.` prefix is stripped).
5. **Execute** is invoked when the user runs an action of a result.
   Each action sits in a fixed slot and carries a command, which the
   gadget set in `search()` or `entries()`. The host passes the
   command of the triggered slot back to `search::execute(command)`.
   See [02-data-types.md](02-data-types.md#execution) for the
   complete type contract and post-action semantics.
6. **Scheduled tasks.** If the manifest declares `[[tasks]]`, the
   bridge spawns a tokio scheduler loop that walks every parsed cron
   expression and invokes `tasks::run-task(task-id)` on the live
   instance. The loop is co-operatively shut down via an
   `AtomicBool` flag on disable.

## Enable / disable gating

The host owns the enabled state (ADR 0025). Each gadget is wrapped in
a `GadgetSlot` that holds an `AtomicBool` for the enabled flag plus
the `CoalescingDispatcher` for `on-setting-changed`. The enabled key
lives at `enabled.<gadget-id>` in the top-level settings namespace,
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
(`src-tauri/src/commands/types.rs`). The frontend merges per-source
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
export. These calls are request/response only, with no streaming,
using JSON-encoded payloads on both sides. See
[04-frontend-reception.md](04-frontend-reception.md) for the full
rendering pipeline and
[05-gadget-messaging.md](05-gadget-messaging.md) for the messaging
contract.

## Host capabilities

Every host import is gated by the manifest. A capability the
manifest doesn't grant is unreachable; the host returns
`permission-denied(...)` (or the interface's equivalent variant) at
call time.

| Interface | Manifest gate | Notes |
|---|---|---|
| `logging` | always | Structured logs + timing spans, routed to host logger. |
| `clipboard` | `[permissions] clipboard = true` | Write-only (`write-text`). Read intentionally not exposed. |
| `settings` | always | Scoped to `gadgets.<id>.*`; JSON-encoded values. |
| `frecency` | `[permissions] frecency = true` | Read-only top-N by score; tracking happens automatically host-side. |
| `assets` | always | Spatial guarantee: paths validated to stay inside the gadget root. |
| `platform` | always | OS / arch detection. |
| `path-resolver` | always | `${...}` substitution against host-resolved paths. |
| `sql-storage` | `[storage.sql]` declared | Per-gadget SQLite at `gadget-home/<id>/sql/storage.sqlite3`. |
| `opener` | `[permissions.opener]` | Per-capability flags: `schemes`, `open-path`, `reveal-path`. |
| `http` | `[permissions.http] origins` | Origin allowlist or `"*"` for trust-all. |
| `filesystem` | `[permissions.filesystem] read` | Globs canonicalized; symlinks resolved before match. |
| `command` | `[[permissions.command]]` | Per-binary argv-shape rules; no shell wrapping. |
| `website-metadata` | `[permissions] website-metadata = true` | Host-shared cache; favicons returned as `entry-icon`. |

The `${gadget-data}`, `${gadget-archive}`, `${home}`,
`${xdg-config}`, `${xdg-data}` substitution variables are recognized
in `[permissions.command]` `path-under` roots, literal argv values,
per-rule `cwd`, and `[permissions.filesystem] read` patterns. See the
relevant ADR (0029–0032, 0037–0040) for each capability's design
rationale.

## SDK ergonomics

`torchsnap-gadget-sdk` (`gadgets/gadget-sdk/src/`) re-exports the
generated bindings and adds higher-level helpers on top of the raw
WIT interfaces:

- `prelude::*` brings the typed `Search` and `Messaging` traits, the
  `LifecycleGuest` and `TasksGuest` guest traits, the SDK's entry
  and action types (`CatalogEntry`, `ScoredEntry`, `SearchResponse`,
  `ViewResponse`, `Actions`, `Action`, `Slot`, `EntryIcon`,
  `PostAction`), the flat host import aliases, and the
  `define_gadget!` / `impl_noop_*!` macros into scope. The generated
  `SearchGuest` and `MessagingGuest` stay at the crate root for
  gadgets that implement them by hand.
- `Search` is generic over the gadget's own `Command` type. The SDK
  encodes each action's command to JSON for the WIT and decodes it
  again before `execute()` runs.
- `Messaging` decodes each frontend call into the gadget's own
  `Request` enum before `handle()` runs.
- `settings::get` / `get_or` / `get_or_else` fold the JSON parse
  into the lookup.
- `sql_storage::Row` plus `query_one` / `query_all` give typed column
  access over the raw `Vec<Vec<sql-value>>` interface.
- `command`, `logging`, `website_metadata` modules wrap
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
impl Search for MyGadget { /* type Command; entries / search / execute */ }
```
