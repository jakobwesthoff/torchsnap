# Gadget Development Guide

This guide is the canonical reference for authoring Torchsnap gadgets. The
gadget system is built on the WebAssembly Component Model: gadgets are
sandboxed WASM components, written in Rust against a fixed WIT interface,
shipped as `.torchsnap` zip archives, and loaded at runtime by the host.

The single source of truth for the gadget contract is the WIT file at
`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`. The Rust SDK
(`gadgets/gadget-sdk/`) generates bindings from it and layers
ergonomic helpers on top. When this guide and the WIT disagree, the
WIT wins.

The gadget API is at version 0.2.0: the WIT package
`torchsnap:gadget@0.2.0`, the Rust SDK crate `torchsnap-gadget-sdk`
and the TypeScript SDK package `@torchsnap/gadget-sdk` share one
version number. When any of the three changes incompatibly, all three
move to the same new version (ADR 0056). Gadgets built against 0.1.0
need code changes and a rebuild.

## Contents

- [Gadget model](#gadget-model)
- [Building a gadget from scratch](#building-a-gadget-from-scratch)
- [The four guest traits](#the-four-guest-traits)
- [Lifecycle and instance management](#lifecycle-and-instance-management)
- [Search: catalog vs query, prefix routing, response variants](#search-catalog-vs-query-prefix-routing-response-variants)
- [Actions and execute()](#actions-and-execute)
- [Messaging (frontend ↔ gadget RPC)](#messaging-frontend--gadget-rpc)
- [Scheduled tasks](#scheduled-tasks)
- [Manifest (`manifest.toml`)](#manifest-manifesttoml)
- [Host APIs (WIT imports)](#host-apis-wit-imports)
- [Frontend components](#frontend-components)
- [Storage layout on disk](#storage-layout-on-disk)
- [Discovery, bundling, and distribution](#discovery-bundling-and-distribution)
- [Worked example: the `calculator` gadget](#worked-example-the-calculator-gadget)

---

## Gadget model

A Torchsnap gadget is a WASM **component** built for the
`wasm32-wasip2` target. It exports four interfaces (`lifecycle`,
`search`, `messaging`, `tasks`) and imports a fixed set of host
capabilities (logging, sql, settings, http, …) defined in the WIT
`gadget` world (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`,
lines 887–907).

The component is shipped inside a **`.torchsnap` zip archive**
containing:

- `manifest.toml` — gadget metadata, settings defaults, declared
  permissions, scheduled tasks, frontend bundles.
- `<id>_gadget.wasm` (or whatever path `manifest.toml`'s
  `gadget.wasm` field points at) — the compiled component.
- Optional asset files (SQL migration scripts, JSON seed data,
  bundled icon images, frontend `dist/` output).

At launcher startup the host scans three roots
(`<resource_dir>/gadgets/`, `<CARGO_MANIFEST_DIR>/../gadgets/` in
debug builds, and `<app_data_dir>/gadgets/`), parses each
manifest, **compiles** every gadget's component, and stages it.
Components are **instantiated lazily on enable** — disabled gadgets
do not pay the per-instance memory cost (ADR 0033). Each enable
produces a fresh wasmtime `Store`; disable drops it.

There is **no `cargo-component`**. Gadget crates depend on the
`torchsnap-gadget-sdk` crate, which owns the
`wit_bindgen::generate!` invocation and re-exports the generated
trait surface. A plain `cargo build --release` from the
`gadgets/` workspace is all that is needed; the
`wasm32-wasip2` target is set as the workspace default via
`gadgets/.cargo/config.toml`.

---

## Building a gadget from scratch

The fastest path is to copy `gadgets/template/`. It is a
working, self-documenting starter that exercises every guest
export, the SQL store, scheduled tasks, settings, messaging,
and a custom frontend view.

### Workspace layout

Every gadget lives under `gadgets/<id>/` and is a member of the
gadget virtual workspace declared in `gadgets/Cargo.toml`. To
add a new gadget:

1. Create `gadgets/<your-id>/` with `Cargo.toml`, `manifest.toml`,
   and `src/lib.rs`.
2. Add `<your-id>` to the `members` list in `gadgets/Cargo.toml`.
3. (Optional) Add a `frontend/` directory if the gadget ships UI
   bundles, plus `migrations/` if it uses SQL storage.

### `Cargo.toml`

Gadget crates are `cdylib` libraries that depend on the SDK by
relative path. The release profile is shared at the workspace
level (`gadgets/Cargo.toml`, `[profile.release]`):

```toml
[package]
name = "mygadget-gadget"
version = "0.1.0"
edition = "2024"

[dependencies]
torchsnap-gadget-sdk = { path = "../gadget-sdk" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[lib]
crate-type = ["cdylib"]
```

Add any third-party crates you need (`evalexpr`, `regex`,
`blake3`, `ulid`, etc.) — they are compiled into the WASM
component like any other dependency. The only constraint is that
they must build for `wasm32-wasip2` (no `mio`, no `tokio`, no
threads).

### `src/lib.rs` skeleton

Every gadget opens with a `define_gadget!` macro call and four
trait implementations. The traits take **no `self` receiver** —
`wasm32-wasip2` is single-threaded and the component holds its
state through `thread_local!` `Cell`s, `LazyLock`s, or static
`OnceCell`s.

```rust
use serde::{Deserialize, Serialize};
use torchsnap_gadget_sdk::prelude::*;

struct MyGadget;
define_gadget!(MyGadget);

impl LifecycleGuest for MyGadget {
    fn enable() {
        log_info!("gadget enabled");
    }
    fn disable() {}
    fn on_setting_changed(_key: String, _value: String) {}
}

/// What this gadget's actions do. The SDK hands the value an
/// action carries back to `execute()` when the user runs it.
#[derive(Serialize, Deserialize)]
enum Command {
    OpenDocs,
}

impl Search for MyGadget {
    type Command = Command;

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse<Command> {
        SearchResponse::Nothing
    }

    fn execute(command: Command) -> Result<PostAction, String> {
        match command {
            Command::OpenDocs => {
                opener::open_url("https://example.com/docs")
                    .map_err(|e| format!("open URL: {e}"))?;
            }
        }
        Ok(PostAction::Dismiss)
    }
}

// Gadgets that don't use messaging or scheduled tasks still have
// to satisfy the WIT contract. The SDK provides one-line stubs.
impl_noop_messaging!(MyGadget);
impl_noop_tasks!(MyGadget);
```

`define_gadget!(MyGadget)` (`gadgets/gadget-sdk/src/lib.rs`)
expands to the `wit_bindgen`-generated `export!` macro, wiring
the type to every component-model FFI shim. `Search` has a default
`entries()` that returns no catalog entries, so a query-only gadget
leaves it out. `impl_noop_messaging!`
and `impl_noop_tasks!` provide the stub implementations of the
two exports a gadget almost always still has to declare even when
it does not use them.

### Building

From anywhere inside the gadget workspace:

```sh
cargo build --release -p mygadget-gadget
```

The `wasm32-wasip2` target is inherited from
`gadgets/.cargo/config.toml`. The output lands at
`gadgets/target/wasm32-wasip2/release/mygadget_gadget.wasm`. The
debug-build loader picks it up directly from your source
directory (see [Discovery](#discovery-bundling-and-distribution));
for shipping, the `just build-gadget` / `just package-gadget`
recipes assemble the `.torchsnap` archive.

---

## The four guest traits

Each of the four WIT exports has one trait a gadget implements. All
four come from `torchsnap_gadget_sdk::prelude`, so gadget code only
ever needs `use torchsnap_gadget_sdk::prelude::*;`:

| Trait              | WIT export      | Methods                                           |
|--------------------|-----------------|---------------------------------------------------|
| `LifecycleGuest`   | `lifecycle`     | `enable`, `disable`, `on_setting_changed`         |
| `Search`           | `search`        | `entries` (optional), `search`, `execute`; associated type `Command` |
| `Messaging`        | `messaging`     | `handle`; associated type `Request`               |
| `TasksGuest`       | `tasks`         | `run_task`                                        |

`LifecycleGuest` and `TasksGuest` are the `wit_bindgen`-generated
guest traits. `Search` and `Messaging` are typed layers the SDK puts
in front of the generated `SearchGuest` and `MessagingGuest`: a
blanket impl provides the generated trait for every type that
implements the typed one, and converts between the gadget's own types
and the JSON strings the WIT carries (see
[Actions and `execute()`](#actions-and-execute) and
[Messaging](#messaging-frontend--gadget-rpc)).

`SearchGuest` and `MessagingGuest` are exported from the crate root,
not from the prelude. A gadget may implement one of them by hand
instead of the typed trait, using the generated entry and response
types from `torchsnap_gadget_sdk::wit_search`. A type cannot
implement both `Search` and `SearchGuest`: the two impls conflict and
the gadget does not compile (E0119). `impl_noop_messaging!` is such
a hand-written `MessagingGuest`.

All methods are **free associated functions on the implementing
type** (no `&self`). State is held in `thread_local!`,
`LazyLock`, or `OnceCell`.

---

## Lifecycle and instance management

ADR 0033 specifies the bridge model: the host **compiles**
every gadget's component once at launcher startup and caches the
resulting `wasmtime::Component`. **Instantiation** (linker setup,
`Store` creation, guest-side initial-state allocation) happens
lazily, on the first enable, and is repeated on every subsequent
re-enable after a disable.

### `enable()`

Called every time the gadget transitions from disabled to
enabled. The host has already created the per-gadget SQLite
database file and applied any declared migrations *before* this
runs, so `sql::connection()` is immediately usable. Typical
work:

- Read settings via `settings::get_or` / `get_or_else`.
- Seed in-memory caches from SQL, bundled assets, or HTTP.
- Update reactive state (`thread_local! { Cell<…> }`) from
  defaults to the user's configured values.

### `disable()`

Called when the user toggles the gadget off, and once on
application exit. The bridge drops the `Store` after this
returns, so any state held in linear memory is reclaimed
automatically — `disable()` only needs to flush anything
not already persisted (most gadgets do nothing).

### `on_setting_changed(key, value)`

Called when a setting under `gadgets.<gadget-id>.*` changes.
`value` is a JSON-encoded string, same encoding as
`settings::get`. Rapid same-key writes are coalesced by the
host's `CoalescingDispatcher` (ADR 0026) before this fires, so
each invocation already carries the latest value — no debouncing
needed on the gadget side.

---

## Search: catalog vs query, prefix routing, response variants

A gadget can serve any combination of three search modes:

| Mode          | Implements              | When to use                                      |
|---------------|-------------------------|--------------------------------------------------|
| Catalog       | `entries()`             | Static/semi-static lists; host fuzzy-matches.    |
| Query (always)| `search()`              | Dynamic results merged with all other gadgets.   |
| Query (prefix)| `search()` + manifest `prefixes` | Exclusive routing when the query starts with the prefix. |

### `entries()`

Returns a `Vec<CatalogEntry<Self::Command>>`. The host runs nucleo fuzzy
matching across `title` and `keywords` and merges the matches
with the rest of the active result list. Called on every
keystroke, so keep it fast and side-effect free.

```rust
fn entries() -> Vec<CatalogEntry<Command>> {
    vec![CatalogEntry {
        id: "my-action".into(),
        title: "My Action".into(),
        subtitle: Some("Does the thing".into()),
        icon: Some(EntryIcon::HeroIcon("bolt".into())),
        keywords: vec!["thing".into()],
        actions: Actions::new().primary("Run", Command::DoTheThing),
    }]
}
```

### `search(query, matched_prefix)`

Returns a `SearchResponse<Self::Command>` variant (the WIT's
`search-response`):

| Variant                 | Effect                                                         |
|-------------------------|----------------------------------------------------------------|
| `Nothing`               | Gadget contributes nothing this keystroke.                     |
| `Results(Vec<ScoredEntry<C>>)` | Pre-scored entries merged into the host's list.         |
| `CustomUi(ViewResponse<C>)`| Host mounts the gadget's named view component, replacing the result list. |
| `InlineUi(ViewResponse<C>)`| Host renders the gadget's inline view above the result list (only one gadget per search may claim the slot). |

`ViewResponse { view, data, results }`:

- `view` resolves against the manifest's `[frontend.views]` /
  `[frontend.inline-views]` map.
- `data` is `Option<String>` carrying a JSON-encoded payload
  (the WIT has no opaque JSON type — gadgets serialize and
  deserialize with `serde_json`).
- `results` is forwarded to the React component as props; use
  it or ignore it depending on whether the view wants to
  render its own list. The view runs an action of one of these
  entries with `onExecute(entryId, slot)` (see
  [Frontend components](#frontend-components)).

### Prefix routing (ADR 0012)

If `manifest.toml` declares `gadget.prefixes = ["="]`, the host
strips the matched prefix and routes the query **exclusively**
to that gadget. Other catalog gadgets and prefix-less query
gadgets are bypassed for the keystroke. The `matched_prefix`
parameter receives the prefix string when this fires (`Some("=")`),
and `None` otherwise.

Longest prefix wins when multiple gadgets register prefixes that
could both match.

### `ScoredEntry`

```rust
pub struct ScoredEntry<C> {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    pub title_highlight_positions: Vec<u32>,    // UTF-16 code unit offsets
    pub subtitle_highlight_positions: Vec<u32>, // UTF-16 code unit offsets
    pub actions: Actions<C>,
}
```

`CatalogEntry<C>`, `ScoredEntry<C>`, `ViewResponse<C>` and
`SearchResponse<C>` are the SDK's own types, generic over the
gadget's command type. The WIT records they map to carry the
commands as strings.

The gadget is responsible for scoring and for computing the
highlight offsets that the launcher renders as bold characters
in the title/subtitle. Frecency bonuses are applied **by the
host** after `search()` returns — gadgets do not need to layer
them in themselves.

### `EntryIcon`

```rust
pub enum EntryIcon {
    HeroIcon(String),    // Heroicon outline name, e.g. "bolt"
    DataUrl(String),     // Inline base64 data URL
    AssetIcon(String),   // Path to a host-cached file
    Emoji(String),       // Single Unicode emoji
}
```

---

## Actions and `execute()`

### Commands

Every action carries a **command**: a value of the gadget's own
`Command` type that says what running the action does. A command
holds whatever `execute()` needs. The bangs gadget, for example,
puts the resolved search URL into `Command::OpenUrl(url)`.

```rust
#[derive(Serialize, Deserialize)]
enum Command {
    OpenUrl(String),
    CopyUrl(String),
}

impl Search for BangsPlugin {
    type Command = Command;

    // search() builds each entry's actions from these commands.

    fn execute(command: Command) -> Result<PostAction, String> {
        match command {
            Command::OpenUrl(url) => {
                opener::open_url(&url).map_err(|e| format!("open URL: {e}"))?;
            }
            Command::CopyUrl(url) => {
                clipboard::write_text(&url).map_err(|e| format!("copy URL to clipboard: {e}"))?;
            }
        }
        Ok(PostAction::Dismiss)
    }
}
```

`Command` must implement `Serialize` and `DeserializeOwned`. The
WIT carries a command as an opaque `string`. The SDK encodes the
command to JSON when entries leave the gadget, and the host stores
it with the entry without inspecting it. When the user runs an
action, the host passes that string back to the WIT `execute`, and
the SDK decodes it before calling `Search::execute`. `execute()`
receives only the command of the triggered action, not the entry id
and not the slot. Commands never reach the frontend.

The SDK handles two failures itself:

- A command that fails to encode in `search()` or `entries()` drops
  its whole entry, and the SDK logs a warning.
- A command that fails to decode in `execute()` returns
  `Err("command decode: …")` without calling `Search::execute`.

A gadget in another language, or one that implements `SearchGuest`
by hand, chooses its own encoding for the command string.

### Slots

An entry's actions are an `Actions<C>`: one optional field per
slot. The slot alone decides the key that runs the action. An empty
slot has no key and no footer hint.

| Slot            | Key                          | Label when `None`   |
|-----------------|------------------------------|---------------------|
| `primary`       | Enter, or a click on the row | the entry's title   |
| `secondary`     | Cmd+Enter                    | the entry's title   |
| `copy`          | Cmd+C                        | "Copy"              |
| `reveal`        | Cmd+Shift+R                  | "Reveal in Finder"  |
| `delete`        | Cmd+Backspace                | "Delete"            |
| `open_settings` | Cmd+,                        | "Open settings"     |

Outside macOS, Ctrl takes the place of Cmd. The default labels are
the same English text on every platform. `primary` and `secondary`
have no default label because their meaning differs per gadget, so
always give them one.

Each action is an `Action<C> { label: Option<String>, command: C }`.
`Action::new(command)` leaves the label to the slot default,
`Action::labeled(label, command)` sets it. Build the slots with the
builder methods, which take a label for `primary` and `secondary`
and an `Action` for the other slots:

```rust
/// A network entry's actions: Enter joins or leaves (`label` says
/// which), Cmd+C copies the id, Cmd+Backspace forgets the network.
fn network_actions(network_id: &str, label: &str) -> Actions<Command> {
    Actions::new()
        .primary(label, Command::Toggle(network_id.to_string()))
        .copy(Action::labeled("Copy network id", Command::CopyId(network_id.to_string())))
        .delete(Action::labeled("Forget", Command::Forget(network_id.to_string())))
}
```

A second builder call for the same slot replaces the first.
`Actions` also has public fields and `Default`, so a struct literal
with `..Default::default()` works too. In a struct literal, filling
a slot twice does not compile. `Actions::iter()` yields the filled
slots as `(Slot, &Action<C>)` in the order of the table above.

The same command may fill several slots. The calculator's history
rows put one `Command::Copy { .. }` into both `primary` and `copy`,
so Enter and Cmd+C both copy the result.

### `execute()` and `PostAction`

`Search::execute` returns `Result<PostAction, String>`:

```rust
pub enum PostAction { Nothing, Dismiss, KeepOpen, OpenSettings }
```

`OpenSettings` hides the launcher and opens the Settings window on
this gadget's section, or on the Gadgets page when the gadget has no
settings section of its own. ZeroTier's "token not configured"
entry uses it: its `primary` action, labeled "Open settings", carries
`Command::OpenSettings`, and `execute()` answers that command with
`PostAction::OpenSettings`.

There is no `ShowCustomUI` variant. A WASM gadget mounts custom UI
only through `SearchResponse::CustomUi` from `search()`.

Returning `Err(string)` rejects the launcher's `search_execute`
call with `gadget execute() returned error: <string>`.

On the host side, the launcher calls `search_execute` with the
entry's source gadget, its id and the slot. The host records the
selection for frecency, looks up the entry from the current search
by `(source, id)`, and passes the command in that slot to the
gadget. If the entry is not in the current search or the slot is
empty, the host logs it and returns `PostAction::Nothing` without
calling the gadget.

---

## Messaging (frontend ↔ gadget RPC)

The WIT `messaging::handle-message` export is the single entry
point a gadget's frontend components call into when they need
backend work done. Launcher views and settings panels share this
one channel.

Implement the SDK's `Messaging` trait with one `Request` enum that
lists every method the frontend may call. The SDK decodes each call
into that enum before `handle` runs:

```rust
/// Every method the calculator frontend calls through `sendMessage`.
#[derive(Deserialize)]
#[serde(tag = "method", content = "payload", rename_all = "snake_case")]
enum Request {
    /// Copy a result to the clipboard and record it in the history.
    Copy(CopyPayload),
    /// Entry count and database size for the settings panel.
    Stats,
    /// Wipe the history table.
    ClearHistory,
}

impl Messaging for CalculatorPlugin {
    type Request = Request;

    fn handle(request: Request) -> Result<serde_json::Value, String> {
        match request {
            Request::Copy(payload) => copy_method(payload),
            Request::Stats => stats_method(),
            Request::ClearHistory => clear_history_method(),
        }
    }
}
```

The SDK deserializes `Request` from
`{"method": <method>, "payload": <payload>}`. With
`#[serde(tag = "method", content = "payload")]`, the method name
selects the variant and the payload fills the variant's fields.
`rename_all` makes the variant names match the method names the
frontend sends: `sendMessage("clear_history", {})` reaches
`Request::ClearHistory`. Methods without arguments are unit
variants. The frontend sends `{}` for them, and the SDK accepts
that payload for a unit variant.

The SDK rejects an unknown method, a payload that does not fit its
variant, or a payload that is not JSON before gadget code runs.
`handle` returns the JSON value the frontend's `sendMessage`
promise resolves with, and `Err(string)` rejects the promise.
`messaging::decode_request::<Request>(method, payload)` runs the
same decoding, so gadget tests can check their request enum against
the payloads their frontend sends.

A gadget can still implement the generated `MessagingGuest` from the
crate root by hand. `torchsnap_gadget_sdk::messaging::parse_payload`
and `to_response` cover the JSON for such an impl. Neither is in the
prelude.

**WASM gadgets cannot stream.** The export is strictly request /
response. The TypeScript `sendMessage(method, payload, onMessage)`
overload exists for symmetry with native gadgets, but WASM-backed
gadgets drop the channel callback silently. See ADR 0030 and the
`GadgetSendMessage` doc comment in
`packages/gadget-sdk/src/shims/hooks.ts` for the rationale and the
future `messaging-stream` sub-interface that would lift the
restriction.

---

## Scheduled tasks

WASM has no thread support, so gadgets cannot run their own
timers. Instead, declare `[[tasks]]` entries in `manifest.toml`
and implement `TasksGuest::run_task` to dispatch by `task_id`:

```toml
[[tasks]]
id = "retention-cleanup"
schedule = "*/30 * * * *"
```

```rust
impl TasksGuest for MyGadget {
    fn run_task(task_id: String) -> Result<(), String> {
        match task_id.as_str() {
            "retention-cleanup" => {
                let db = sql::connection();
                db.execute(
                    "DELETE FROM history WHERE created_at < datetime('now', '-30 days')",
                    &[],
                ).map_err(|e| format!("delete: {e}"))?;
                Ok(())
            }
            other => Err(format!("unknown task: {other}")),
        }
    }
}
```

`schedule` is a 5-field POSIX cron expression
(`minute hour day month weekday`); sub-minute scheduling is not
supported. The host runs a per-gadget tokio scheduler that walks
every entry, sleeps until the earliest next fire, and invokes
`run-task` on the configured cadence (ADR 0032).

Semantics:

- **First fire is at the next cron match.** No automatic
  immediate fire on enable. Gadgets that want startup work
  should do it in `enable()`.
- **Failures are logged, not fatal.** Returning `Err(string)`
  is logged at error level and does not auto-disable the
  gadget.
- **Missed fires across launcher restarts are not made up.**
- **Sequential execution.** Two tasks scheduled at the same
  instant run back-to-back in manifest order; the wasmtime
  store mutex serializes every guest call.
- **Gadgets without `[[tasks]]` pay zero cost** — the host
  never spawns the scheduler loop. `impl_noop_tasks!(MyGadget);`
  is enough to satisfy the WIT contract.

---

## Manifest (`manifest.toml`)

The full schema is in `src-tauri/src/wasm/manifest.rs`. Every
table is documented inline there.

### `[gadget]` (required)

```toml
[gadget]
id          = "mygadget"            # lowercase a-z 0-9 hyphens; no leading/trailing -
name        = "My Gadget"
description = "Short one-liner"
version     = "0.1.0"
wasm        = "mygadget_gadget.wasm" # gadget-relative path to the component
icon        = "heroicons:bolt"       # or a gadget-relative path to a WebP image
prefixes    = ["="]                  # optional — exclusive query routing
```

`id` validation is enforced in `manifest.rs:171`; the same string
becomes the SQL namespace, settings prefix, frecency scope, data
directory name, and frontend registry key.

### `[settings]`

Default values applied to the settings store on first load.
Existing user values are never overwritten. Values are arbitrary
JSON-compatible scalars / structures.

```toml
[settings]
heuristicEnabled = true
retentionDays    = 30
```

### `[storage.sql]`

Per-gadget SQLite database. The host creates the database file
and applies migrations *before* the guest's `enable()` runs.

```toml
[storage.sql]
migrations = [
    "migrations/001_init.sql",
    "migrations/002_add_index.sql",
]
```

Each path is gadget-relative. Files are plain SQL — diffable,
shareable with unit tests via `include_str!`, and `query_one` /
`query_all` helpers in the SDK consume the host's response rows.

### `[[tasks]]`

Repeat-table form for scheduled background tasks. Fields:

- `id` — must be unique within the manifest; passed back to
  `run_task`.
- `schedule` — 5-field POSIX cron expression.

### `[frontend]`

Frontend bundle declarations. Omit entirely if the gadget has no
UI. All paths are gadget-relative.

```toml
[frontend]
launcher-bundle = "frontend/dist/launcher.js"
launcher-css    = "frontend/dist/launcher.css"
settings-bundle = "frontend/dist/settings.js"
settings-css    = "frontend/dist/settings.css"

[frontend.views]
history = "CalculatorView"

[frontend.inline-views]
result = "CalculatorInline"

[frontend.settings]
component = "CalculatorSettings"
```

`[frontend.views]` and `[frontend.inline-views]` map view names
(used in `SearchResponse::CustomUi { view: "history", … }`) to
named exports from the launcher bundle. `[frontend.settings]
component` names the export the settings webview mounts.

### `[permissions]`

Capability opt-ins. **Deny by default**: a permission table that
is omitted entirely means the gadget has no access to that
interface. Detail tables:

```toml
[permissions]
website-metadata = true              # opt into the website metadata cache

[permissions.opener]
schemes      = ["https", "http"]     # for opener::open_url
open-path    = true                  # for opener::open_path
reveal-path  = true                  # for opener::reveal_path

[permissions.http]
origins = ["https://api.example.com"]   # or ["*"] for trust-all

[permissions.filesystem]
read = [
    "${xdg-config}/myapp/config.toml",
    "/var/lib/myapp/data/*.json",
]

[[permissions.command]]
binary = "git"
argv   = [
    { kind = "literal", value = "log" },
    { kind = "rest",    constraint = { kind = "any-string" } },
]
```

The argv-constraint vocabulary (`literal`, `enum`, `glob`,
`regex`, `path-under`, `any-string`, `rest`) is documented in
`manifest.rs:570–610` and ADR 0040. `${gadget-data}`,
`${gadget-archive}`, `${home}`, `${xdg-config}`, `${xdg-data}`
substitution applies in both `[permissions.filesystem] read` patterns
and `[[permissions.command]]` `path-under` roots / per-rule
`cwd`.

### `[shortcuts]` (declarative only — currently inert for WASM)

`manifest.rs:54` accepts a `[shortcuts]` table but the WIT does
not currently expose a `handle-shortcut` guest export, so any
declared shortcuts are parsed and stored but never fire for
WASM gadgets. Native (built-in) gadgets still use them. Treat
this section as reserved for a future API; do not plan around
it.

### Path-safety rules

Every gadget-relative path the manifest references (`wasm`,
asset `icon`, `frontend.*-bundle`, `frontend.*-css`, every
migration path) must satisfy `validate_gadget_path`:

- No `..` segments.
- No leading `/`.
- No backslashes.
- No Windows drive letters.
- No NUL bytes.

A manifest that violates any of these fails to parse and the
gadget never loads.

---

## Host APIs (WIT imports)

The WIT `gadget` world (`torchsnap-gadget.wit:887–907`) imports
fourteen interfaces. The SDK re-exports each one — either flat
under `torchsnap_gadget_sdk::prelude::*` (`logging`, `clipboard`,
`sql`, `frecency`, `settings`, `opener`, `http`, `fs`, `assets`,
`command`, `platform`, `paths`, `messaging`, `website_metadata`)
or under a `_host` alias when the SDK adds an ergonomic wrapper
that would otherwise shadow the raw bindings.

### `logging`

Structured logging with optional spans. The SDK's
`gadgets/gadget-sdk/src/logging.rs` provides `log_info!`,
`log_warn!`, `log_error!`, `log_debug!`, `log_trace!` macros
modelled on `tracing`:

```rust
log_info!("user toggled feature", "feature" => "history", "enabled" => true);
```

For explicit timing spans, call `logging::span_start` /
`logging::span_end` directly.

### `clipboard`

Single operation: `clipboard::write_text(text) -> Result<(), String>`.
Read access is intentionally not exposed — sandboxed gadgets must
not be able to slurp arbitrary host clipboard contents (WIT
lines 35–55).

### `sql` — per-gadget SQLite

Available only when the manifest declares `[storage.sql]`.
`sql::connection()` is **infallible** for gadgets that opted in
(the host has already initialized the database). The SDK's
`sql::query_one` / `sql::query_all` collect rows into a typed
`Row` newtype with `integer` / `text` / `real` / `blob` /
`is_null` accessors. `From` impls on `SqlValue` cover the common
parameter types:

```rust
use torchsnap_gadget_sdk::sql::{query_all, SqlValue};

let db = sql::connection();
db.execute(
    "INSERT INTO history (expression, result) VALUES (?, ?)",
    &[SqlValue::from(expr), SqlValue::from(result)],
).map_err(|e| format!("insert: {e}"))?;

let rows = query_all(&db, "SELECT id, expression FROM history ORDER BY id DESC LIMIT 50", &[])?;
for row in &rows {
    if let (Some(id), Some(expr)) = (row.integer(0), row.text(1)) {
        log_info!("history row", "id" => id, "expr" => expr);
    }
}
```

The WIT `sql-value` variant deliberately omits the host's
internal `List` variant — gadgets build their own `IN (?, ?, ?)`
clauses (one `?` per element) before calling `query` / `execute`.
Transactions are not currently exposed; raise the API gap if a
gadget needs them.

> The WIT doc-comment on `interface sql` (`torchsnap-gadget.wit:60-61`)
> states the database file lives at
> `app_data_dir/gadgets/<gadget-id>/storage.db`. That comment is
> stale. The actual path used by the bridge is
> `<app_data_dir>/gadget-home/<gadget-id>/sql/storage.sqlite3`
> (see [Storage layout on disk](#storage-layout-on-disk)).

### `frecency`

Read-only. The host already records selections automatically
before dispatching `execute()` and applies score bonuses to
`search()` results — neither requires gadget code. The interface
exposes only:

```rust
frecency::is_enabled() -> bool
frecency::top_items(limit: u32) -> Vec<FrecencyItem>
```

Use `top_items` to drive empty-query "most-used first" ordering
in custom UIs (e.g. the emoji picker's browse mode). There is
**no `record` operation** in the WIT — gadgets cannot push
synthetic frecency events from custom UIs.

### `settings`

Gadget-namespaced reads only. The raw WIT (`settings::get(key) -> Option<String>`)
returns JSON-encoded values; the SDK's
`gadgets/gadget-sdk/src/settings.rs` wraps it:

```rust
let verbose: bool = settings::get_or("verbose", false);
let greeting: String = settings::get_or_else("greeting", || "(unset)".into());
```

Writes from the gadget side are deliberately not exposed — the
launcher's settings UI is the single source of mutation. The
gadget reacts to changes through `LifecycleGuest::on_setting_changed`.

### `opener`

URL and path opening through the OS handler chain. Three
operations, each gated by a separate manifest field under
`[permissions.opener]`:

```toml
[permissions.opener]
schemes      = ["https", "http"]
open-path    = true
reveal-path  = true
```

```rust
opener::open_url("https://example.com")?;
opener::open_path("/Applications/Calendar.app")?;     // requires open-path=true
opener::reveal_path("/Users/me/Documents/report.pdf")?; // requires reveal-path=true
```

Errors come back as the `OpenerError` variant
(`PermissionDenied(String)`, `InvalidUrl(String)`,
`BackendFailure(String)`).

### `http`

Synchronous HTTP client. Declare allowed origins:

```toml
[permissions.http]
origins = ["https://api.duckduckgo.com"]    # or ["*"] for trust-all
```

```rust
use torchsnap_gadget_sdk::http::{HttpMethod, HttpRequest};

let response = http::fetch(&HttpRequest {
    url: "https://api.duckduckgo.com/?q=rust&format=json".into(),
    method: HttpMethod::Get,
    headers: vec![],
    body: None,
    timeout_ms: Some(5_000),
    max_body_size: None,
    insecure_tls: false,
})?;
```

Origins are normalized (`url::Origin::ascii_serialization`) at
manifest parse time, so spelling variants (`https://x/`,
`HTTPS://X`, `https://x:443`) all collapse to the same canonical
form. HTTP status codes (`4xx`, `5xx`) are **not errors** —
`HttpError` covers only transport-level failures
(`ConnectionRefused`, `Timeout`, `DnsFailed`, `TlsFailed`,
`InvalidUrl`, `Other`, `PermissionDenied`).

`insecure_tls: true` disables certificate validation — needed
for self-signed local daemons (Docker over TLS, k3s,
zerotier-one). The origin allowlist still gates which endpoint
the gadget can reach.

### `fs`

Read-only filesystem access through a manifest-declared
allowlist:

```toml
[permissions.filesystem]
read = [
    "${xdg-config}/myapp/config.toml",
    "/var/lib/myapp/data/*.json",
]
```

Glob syntax: `*` matches a single path segment, `**` matches
across segments. `?`, character classes, and brace alternation
are intentionally not supported.

Three operations:

```rust
fs::read_file(path) -> Result<Vec<u8>, FsError>
fs::file_exists(path) -> bool                  // false on any failure
fs::metadata(path) -> Result<FileMetadata, FsError>
```

The host canonicalizes the requested path (resolving symlinks
to their real target) before matching against the canonicalized
manifest allowlist — a symlink that points outside the allow
set is rejected because the resolved real path no longer
matches.

### `assets`

Read files from inside the gadget's own `.torchsnap` archive (or
development directory). **No manifest opt-in** — every gadget
can read its own bundled files. Spatial isolation is enforced
by `validate_gadget_path`.

```rust
use torchsnap_gadget_sdk::assets::AssetsError;

match assets::read("data/seed.json") {
    Ok(bytes) => /* parse and use */,
    Err(AssetsError::NotFound) => /* optional asset, fall back */,
    Err(AssetsError::InvalidPath(msg)) => /* programmer bug */,
    Err(AssetsError::IoError(msg)) => /* archive corrupted */,
}
```

`assets::exists(path) -> Result<bool, AssetsError>` is cheap
(central-directory lookup for archives, `stat` for directories)
and useful for optional bundle-conditional fallbacks. Bytes
cross the WIT boundary in full on every `read`; gadgets that
need to keep an asset around should hold one copy in their own
linear memory after the first read.

### `command`

Run a system process. Gadgets declare repeated
`[[permissions.command]]` rules; each `run` call must match
exactly one rule. The SDK's
`gadgets/gadget-sdk/src/command.rs` provides a builder:

```rust
use std::time::Duration;
use torchsnap_gadget_sdk::command;

let result = command::run("git")
    .arg("log")
    .arg("--oneline")
    .arg("-n").arg("10")
    .timeout(Duration::from_secs(5))
    .invoke()?;
let stdout = String::from_utf8_lossy(&result.stdout);
```

The matcher is pure and the manifest validator rejects any pair
of rules whose argv shapes could both accept the same call, so
permission decisions never silently depend on rule ordering. No
shell, no PTY, no streaming; the host's store mutex serializes
calls. See ADR 0040 for the full trust model.

`opener`-class binaries (`open`, `xdg-open`, `start`, `cmd /c
start`) are rejected at manifest parse time — gadgets that want
"open with the registered application" use
`[permissions.opener] open-path = true` instead.

### `platform`

Runtime OS / arch detection. WASI does not expose either to
guests, and gadgets making platform-conditional decisions
(different binaries, different default paths, OS-gated
features) need the signal:

```rust
match platform::current_os() {
    platform::Os::Macos => /* … */,
    platform::Os::Linux => /* … */,
    platform::Os::Windows => /* … */,
    platform::Os::Other(name) => /* … */,
}
```

No permission required.

### `paths`

Resolve `${...}` substitution variables at runtime. Useful when
constructing argv strings that must satisfy a `path-under`
constraint referencing host-resolved paths:

```rust
let helper = paths::resolve("${gadget-archive}/bin/helper")?;
let result = command::run(&helper).arg("status").invoke()?;
```

Recognized variables: `gadget-data`, `gadget-archive`, `home`,
`xdg-config`, `xdg-data`.

### `website-metadata`

Shared host cache for per-domain title / description /
favicon. Gate with `[permissions]\nwebsite-metadata = true`.
The SDK's `gadgets/gadget-sdk/src/website_metadata.rs` flattens
the WIT's nested `Result<LookupResult, WebsiteMetadataError>`
into a `Metadata` enum and folds the (programmer-bug) error
variants into `Unreachable` after logging:

```rust
let icon = website_metadata::favicon_or(
    "example.com",
    EntryIcon::HeroIcon("globe-alt".into()),
);
```

`lookup_cached` never blocks; on cold miss it returns
`Pending` and schedules a background fetch (the next call
sees the populated cache). `lookup_blocking` waits for the
fetch (or coalesces with any in-flight request for the same
domain). In a per-keystroke `search()` loop, prefer
`lookup_cached`.

---

## Frontend components

Gadget frontends are React components bundled with Vite and
loaded into the launcher / settings webviews via dynamic
`import()` over the host-side `torchsnap-gadget://` URL scheme
(ADR 0028).

### `@torchsnap/gadget-sdk` package

Gadgets declare a `file:`-protocol dependency on the in-tree
SDK package at `packages/gadget-sdk/`. Subpath exports
(`packages/gadget-sdk/package.json`):

| Subpath                            | What it exports                                                |
|------------------------------------|----------------------------------------------------------------|
| `@torchsnap/gadget-sdk`            | Type-only: `GadgetViewProps`, `InlineViewProps`, `GadgetSettingsProps`, `ActionSlot`, `Action`, `EntryIcon`, `SourcedEntry`, `FooterHint`, `FooterState`, `Logger`, `LogLevel`, `UsePluginSetting`. |
| `@torchsnap/gadget-sdk/hooks`      | The four host hooks (see below).                               |
| `@torchsnap/gadget-sdk/components` | Shared launcher building blocks.                               |
| `@torchsnap/gadget-sdk/keybindings`| Keybinding helper utilities.                                   |
| `@torchsnap/gadget-sdk/utils`      | Misc helpers.                                                  |
| `@torchsnap/gadget-sdk/testing`    | Test-only mocks.                                               |
| `@torchsnap/gadget-sdk/vite`       | The `torchsnap()` Vite plugin.                                 |
| `@torchsnap/gadget-sdk/theme.css`  | Shared design-token CSS.                                       |

### `vite.config.ts`

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { torchsnap } from "@torchsnap/gadget-sdk/vite";

export default defineConfig({
  plugins: [torchsnap(), react()],
  build: {
    outDir: "dist",
    lib: { entry: { launcher: "src/launcher.ts", settings: "src/settings.ts" }, formats: ["es"] },
    rollupOptions: { external: ["react", "react/jsx-runtime"] },
  },
});
```

The `torchsnap()` plugin
(`packages/gadget-sdk/src/vite/index.ts`) aliases `react` and
`react/jsx-runtime` to SDK shims that read the host-supplied
React off `window.__torchsnap.React`. Gadget bundles ship only
import declarations — the React runtime stays in the host.

### Component contract (ADR 0028)

The host wraps every gadget mount in a `<GadgetContextProvider>`
that supplies identity, runtime capabilities, launcher actions,
and reactive setting accessors. Gadget components receive
**only** per-render data through their props; everything else
flows through hooks. Sub-components extracted from a gadget
component can call hooks directly — no prop threading required.

The four hooks (`packages/gadget-sdk/src/shims/hooks.ts`):

```tsx
import {
  useGadgetInfo,
  useGadgetRuntime,
  useLauncher,
  useGadgetSetting,
} from "@torchsnap/gadget-sdk/hooks";

import type { GadgetViewProps } from "@torchsnap/gadget-sdk";

export function MyView({ data, results, query, matchedPrefix }: GadgetViewProps) {
  const { id, enabled } = useGadgetInfo();              // identity (always)
  const { sendMessage, logger } = useGadgetRuntime();   // RPC + logging (always)
  const { dismiss, onExecute, openSettings, onFooterChange } = useLauncher(); // launcher tree only
  const [retentionDays, setRetentionDays] = useGadgetSetting<number>("retentionDays");
  // ...
}
```

`useLauncher` throws when called from a settings panel; the
other three are safe everywhere.

A view runs an action of an entry it received in `results` with
`onExecute(entryId, slot)`, where `slot` is an `ActionSlot`:
`"primary"`, `"secondary"`, `"copy"`, `"reveal"`, `"delete"` or
`"openSettings"`. The calculator's history view calls
`onExecute(entry.id, "copy")` when a history row is clicked.
`onExecute` only reaches entries of the current search: for an id
the gadget did not return from that search, the host logs the miss
and does nothing. A view that acts on its own state calls its backend with
`sendMessage` and then a launcher action such as `dismiss()`. The
calculator copies a freshly evaluated result that way.

`openSettings()` opens the Settings window on the gadget's own
section and closes the launcher, the same effect as the
`open-settings` post-action from `execute()`. The returned promise
rejects, leaving the launcher open, when the window cannot open.

### Per-render props

| Prop kind            | Carries                                                       |
|----------------------|---------------------------------------------------------------|
| `GadgetViewProps`    | `results`, `data`, `query`, `matchedPrefix`                   |
| `InlineViewProps`    | `data`, `query`, `matchedPrefix`, `selected`                  |
| `GadgetSettingsProps`| (empty — every value comes from hooks)                        |

`data` is whatever the gadget's `search()` set on
`ViewResponse.data` (a JSON-encoded string the host decodes
before passing). The gadget defines its own request / response
shapes.

### `sendMessage` is one-shot for WASM gadgets

```ts
const result = await sendMessage<Req, Resp>("lookup", { domain: "example.com" });
```

The `onMessage` streaming callback is silently dropped by the
WASM bridge (see `GadgetSendMessage` in
`packages/gadget-sdk/src/shims/hooks.ts`).

### Icons (ADR 0027)

`EntryIcon` is a string-tagged enum on the WIT side. The
launcher resolves `HeroIcon("bolt")` against the embedded
Heroicons set; `DataUrl` and `AssetIcon` render directly;
`Emoji` renders as glyph text.

---

## Storage layout on disk

Per-gadget host-managed state lives under
`<app_data_dir>/gadget-home/<gadget-id>/`, separate from
gadget **code** which lives under
`<app_data_dir>/gadgets/`. SQLite databases use the
`.sqlite3` extension project-wide.

| Path                                                              | Owner           |
|-------------------------------------------------------------------|-----------------|
| `<app_data_dir>/gadgets/<id>.torchsnap`                           | User-installed gadget archives. |
| `<app_data_dir>/gadget-home/<id>/sql/storage.sqlite3`             | Per-gadget SQLite database. |
| `<app_data_dir>/gadget-home/<id>/exec-cwd/`                       | Default cwd for `command::run` calls. |
| `<resource_dir>/gadgets/`                                         | App-bundle gadgets (read-only). |

Uninstalling a user gadget removes both `gadgets/<id>.torchsnap`
and `gadget-home/<id>/` on the next start, before any gadget loads.
Until then the gadget keeps running and the uninstall can be undone.
See ADR 0035 (distribution), ADR 0051 (install review) and ADR 0018
(SQL storage).

---

## Discovery, bundling, and distribution

### Discovery roots (ADR 0035)

The host scans three roots at startup, in precedence order:

1. **System** — `<resource_dir>/gadgets/`. Shipped inside the
   app bundle; populated at build time from
   `gadgets/bundled.toml`.
2. **Dev** — `<CARGO_MANIFEST_DIR>/../gadgets/`. Debug builds
   only; release builds never touch this root.
3. **User** — `<app_data_dir>/gadgets/`. Where user-installed
   `.torchsnap` archives land.

A gadget id can only be registered once; the first root to
claim a given id wins. Inside a single root, an archive
(`foo.torchsnap`) wins over a sibling directory (`foo/`).

The host tags each loaded gadget with a `GadgetSourceKind`
(`system`, `dev`, `user`, `builtin`); the Gadgets settings
panel renders this as a badge and gates the uninstall action
to `user` only.

### Shipping with the app

The release bundle includes only gadgets listed in
`gadgets/bundled.toml`. To add one:

1. Add the gadget id (a directory name under `gadgets/`) to the
   list.
2. Run `just build`.

The `stage-bundled-gadgets` recipe rebuilds each listed gadget,
packages it as `.torchsnap`, copies it into
`target/bundled-gadgets/`, and Tauri picks it up via the
`resources` entry in `tauri.conf.json`.

The repo-root `target/` directory is gitignored and owned
entirely by this staging flow. Cargo itself uses
`src-tauri/target/` (host) and `gadgets/target/` (gadget
workspace) — those are unrelated.

### Sharing a gadget with users

Send users the `.torchsnap` file. They open it in Torchsnap:
double-click in Finder, the Gadgets settings panel (file picker
or drag-drop), or its path on the command line. Torchsnap stages
a copy (archives over 16 MiB are rejected), shows a review with
the gadget's details and every declared permission, and installs
it to `<app_data_dir>/gadgets/<id>.torchsnap` when the user
confirms. The gadget loads after a restart (ADR 0051).

To ship an update, keep the `id`, raise the `version` (compared
as semver to label an update or an older version) and only append
SQL migrations. An archive with fewer migrations than the
installed version is refused. The review marks permissions the
installed version did not have as new.

In a debug build every gadget under `gadgets/` loads as a Dev
gadget, and installing an archive with a Dev gadget's id is
refused. Try the install flow in a release build.

### Trust model (ADR 0036)

There is no archive signing today. The sandbox is the WIT
capability surface: no ambient network access, read-only
filesystem through a manifest allowlist, write-only clipboard,
no thread spawning, no arbitrary process exec (only
manifest-declared rules). Users trust the source they got the
archive from; the host limits what the gadget can reach beyond
that. ADR 0036 documents the rationale and the triggers for
revisiting.

---

## Worked example: the `calculator` gadget

`gadgets/calculator/` exercises the full gadget authoring
surface in one place: prefix routing, custom UI, inline UI,
SQL persistence, scheduled retention, and reactive settings.

### Manifest

```toml
# gadgets/calculator/manifest.toml
[gadget]
id          = "calculator"
name        = "Calculator"
description = "Evaluate math expressions with history tracking"
version     = "0.1.0"
wasm        = "calculator_gadget.wasm"
icon        = "heroicons:calculator"
prefixes    = ["="]                            # exclusive routing on `=`

[settings]
heuristicEnabled = true
historyEnabled   = true
retentionDays    = 30

[storage.sql]
migrations = ["migrations/001_init.sql"]

[[tasks]]
id       = "retention-cleanup"
schedule = "*/30 * * * *"                      # every 30 minutes

[frontend]
launcher-bundle = "frontend/dist/launcher.js"
launcher-css    = "frontend/dist/launcher.css"
settings-bundle = "frontend/dist/settings.js"
settings-css    = "frontend/dist/settings.css"

[frontend.views]
history = "CalculatorView"                     # SearchResponse::CustomUi

[frontend.inline-views]
result  = "CalculatorInline"                   # SearchResponse::InlineUi

[frontend.settings]
component = "CalculatorSettings"
```

### Gadget code (highlights)

State lives in `thread_local!` `Cell`s — `wasm32-wasip2` is
single-threaded, so plain interior mutability is sufficient.
`enable()` seeds them from settings; `on_setting_changed`
refreshes them when the user toggles the setting:

```rust
// gadgets/calculator/src/lib.rs (excerpt)

thread_local! {
    static HEURISTIC_ENABLED: Cell<bool> = const { Cell::new(true) };
    static HISTORY_ENABLED:   Cell<bool> = const { Cell::new(true) };
    static RETENTION_DAYS:    Cell<u32>  = const { Cell::new(30) };
}

impl LifecycleGuest for CalculatorPlugin {
    fn enable() {
        HEURISTIC_ENABLED.with(|c| c.set(settings::get_or("heuristicEnabled", true)));
        HISTORY_ENABLED.with(|c|  c.set(settings::get_or("historyEnabled", true)));
        RETENTION_DAYS.with(|c|   c.set(settings::get_or("retentionDays", 30u32)));
    }
    fn disable() {}
    fn on_setting_changed(key: String, _value: String) {
        // Re-read from the host store — the value arg is informational.
        match key.as_str() {
            "heuristicEnabled" => HEURISTIC_ENABLED.with(|c| c.set(settings::get_or("heuristicEnabled", true))),
            "historyEnabled"   => HISTORY_ENABLED.with(|c|   c.set(settings::get_or("historyEnabled", true))),
            "retentionDays"    => RETENTION_DAYS.with(|c|    c.set(settings::get_or("retentionDays", 30u32))),
            _ => {}
        }
    }
}
```

`search()` dispatches by `matched_prefix`:

- `Some("=")` → render the calculator's full custom UI (history
  list below the inline result).
- `None` and the query looks like math (heuristic regex match) →
  render the inline result above the standard list.
- Otherwise → contribute nothing.

```rust
fn search(query: String, matched_prefix: Option<String>) -> SearchResponse<Command> {
    if matched_prefix.is_some() {
        let payload = json!({ "expression": query, "result": evaluate(&query) });
        let history = query_history(&sql_storage::connection(), &query);
        return SearchResponse::CustomUi(ViewResponse {
            view:    "history".into(),
            data:    Some(payload.to_string()),
            results: history,
        });
    }
    if HEURISTIC_ENABLED.with(|c| c.get()) && looks_like_math(&query) {
        let payload = json!({ "expression": query, "result": evaluate(&query) });
        return SearchResponse::InlineUi(ViewResponse {
            view:    "result".into(),
            data:    Some(payload.to_string()),
            results: vec![],
        });
    }
    SearchResponse::Nothing
}
```

The history rows in `results` are `ScoredEntry<Command>` values.
Each one carries `Command::Copy { expression, result }` in its
`primary` slot, labeled "Copy to Clipboard", and in its `copy` slot.
`execute()` answers that command by copying the result to the
clipboard and, with history enabled, recording it in the history
again.

The retention task piggy-backs on the SQL store and the
manifest-declared cron schedule:

```rust
impl TasksGuest for CalculatorPlugin {
    fn run_task(task_id: String) -> Result<(), String> {
        match task_id.as_str() {
            "retention-cleanup" => {
                let days = RETENTION_DAYS.with(|c| c.get());
                let db = sql::connection();
                db.execute(
                    "DELETE FROM history WHERE created_at < datetime('now', ?)",
                    &[SqlValue::from(format!("-{days} days"))],
                ).map_err(|e| format!("delete: {e}"))?;
                Ok(())
            }
            other => Err(format!("unknown task: {other}")),
        }
    }
}
```

On Enter, both launcher views copy the evaluated result with
`useGadgetRuntime().sendMessage("copy", { expression, result,
resultType })` and then call `useLauncher().dismiss()`. Enter on a
selected history row, or a click on one, runs that row's `copy`
action with `useLauncher().onExecute(entry.id, "copy")`. The settings panel
renders its slider from `useGadgetSetting<number>("retentionDays")`
and sends `stats` and `clear_history`. The Rust `Messaging` impl
decodes these calls into its `Request` enum (`Copy`, `Stats`,
`ClearHistory`).

The full source is at `gadgets/calculator/src/lib.rs`; reading
it end-to-end is the fastest way to see every piece in
context.
