# Plugin Development Guide

This guide is the canonical reference for authoring Torchsnap plugins. The
plugin system is built on the WebAssembly Component Model: plugins are
sandboxed WASM components, written in Rust against a fixed WIT interface,
shipped as `.torchsnap` zip archives, and loaded at runtime by the host.

The single source of truth for the plugin contract is the WIT file at
`plugins/plugin-sdk/wit/torchsnap-plugin.wit`. The Rust SDK
(`plugins/plugin-sdk/`) generates bindings from it and layers
ergonomic helpers on top. When this guide and the WIT disagree, the
WIT wins.

## Contents

- [Plugin model](#plugin-model)
- [Building a plugin from scratch](#building-a-plugin-from-scratch)
- [The four guest traits](#the-four-guest-traits)
- [Lifecycle and instance management](#lifecycle-and-instance-management)
- [Search: catalog vs query, prefix routing, response variants](#search-catalog-vs-query-prefix-routing-response-variants)
- [Actions and execute()](#actions-and-execute)
- [Messaging (frontend ↔ plugin RPC)](#messaging-frontend--plugin-rpc)
- [Scheduled tasks](#scheduled-tasks)
- [Manifest (`manifest.toml`)](#manifest-manifesttoml)
- [Host APIs (WIT imports)](#host-apis-wit-imports)
- [Frontend components](#frontend-components)
- [Storage layout on disk](#storage-layout-on-disk)
- [Discovery, bundling, and distribution](#discovery-bundling-and-distribution)
- [Worked example: the `calculator` plugin](#worked-example-the-calculator-plugin)

---

## Plugin model

A Torchsnap plugin is a WASM **component** built for the
`wasm32-wasip2` target. It exports four interfaces (`lifecycle`,
`search`, `messaging`, `tasks`) and imports a fixed set of host
capabilities (logging, sql, settings, http, …) defined in the WIT
`plugin` world (`plugins/plugin-sdk/wit/torchsnap-plugin.wit`,
lines 887–907).

The component is shipped inside a **`.torchsnap` zip archive**
containing:

- `manifest.toml` — plugin metadata, settings defaults, declared
  permissions, scheduled tasks, frontend bundles.
- `<id>_plugin.wasm` (or whatever path `manifest.toml`'s
  `plugin.wasm` field points at) — the compiled component.
- Optional asset files (SQL migration scripts, JSON seed data,
  bundled icon images, frontend `dist/` output).

At launcher startup the host scans three roots
(`<resource_dir>/plugins/`, `<CARGO_MANIFEST_DIR>/../plugins/` in
debug builds, and `<app_data_dir>/plugins/`), parses each
manifest, **compiles** every plugin's component, and stages it.
Components are **instantiated lazily on enable** — disabled plugins
do not pay the per-instance memory cost (ADR 0033). Each enable
produces a fresh wasmtime `Store`; disable drops it.

There is **no `cargo-component`**. Plugin crates depend on the
`torchsnap-plugin-sdk` crate, which owns the
`wit_bindgen::generate!` invocation and re-exports the generated
trait surface. A plain `cargo build --release` from the
`plugins/` workspace is all that is needed; the
`wasm32-wasip2` target is set as the workspace default via
`plugins/.cargo/config.toml`.

---

## Building a plugin from scratch

The fastest path is to copy `plugins/template/`. It is a
working, self-documenting starter that exercises every guest
export, the SQL store, scheduled tasks, settings, messaging,
and a custom frontend view.

### Workspace layout

Every plugin lives under `plugins/<id>/` and is a member of the
plugin virtual workspace declared in `plugins/Cargo.toml`. To
add a new plugin:

1. Create `plugins/<your-id>/` with `Cargo.toml`, `manifest.toml`,
   and `src/lib.rs`.
2. Add `<your-id>` to the `members` list in `plugins/Cargo.toml`.
3. (Optional) Add a `frontend/` directory if the plugin ships UI
   bundles, plus `migrations/` if it uses SQL storage.

### `Cargo.toml`

Plugin crates are `cdylib` libraries that depend on the SDK by
relative path. The release profile is shared at the workspace
level (`plugins/Cargo.toml`, `[profile.release]`):

```toml
[package]
name = "myplugin-plugin"
version = "0.1.0"
edition = "2024"

[dependencies]
torchsnap-plugin-sdk = { path = "../plugin-sdk" }
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

Every plugin opens with a `define_plugin!` macro call and four
trait implementations. The traits take **no `self` receiver** —
`wasm32-wasip2` is single-threaded and the component holds its
state through `thread_local!` `Cell`s, `LazyLock`s, or static
`OnceCell`s.

```rust
use torchsnap_plugin_sdk::prelude::*;

struct MyPlugin;
define_plugin!(MyPlugin);

impl LifecycleGuest for MyPlugin {
    fn enable() {
        log_info!("plugin enabled");
    }
    fn disable() {}
    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for MyPlugin {
    fn entries() -> Vec<CatalogEntry> { Vec::new() }
    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }
    fn execute(_entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        Ok(PostAction::Dismiss)
    }
}

// Plugins that don't use messaging or scheduled tasks still have
// to satisfy the WIT contract. The SDK provides one-line stubs.
impl_noop_messaging!(MyPlugin);
impl_noop_tasks!(MyPlugin);
```

`define_plugin!(MyPlugin)` (`plugins/plugin-sdk/src/lib.rs:150`)
expands to the `wit_bindgen`-generated `export!` macro, wiring
the type to every component-model FFI shim. `impl_noop_messaging!`
and `impl_noop_tasks!` provide the stub implementations of the
two exports a plugin almost always still has to declare even when
it does not use them.

### Building

From anywhere inside the plugin workspace:

```sh
cargo build --release -p myplugin-plugin
```

The `wasm32-wasip2` target is inherited from
`plugins/.cargo/config.toml`. The output lands at
`plugins/target/wasm32-wasip2/release/myplugin_plugin.wasm`. The
debug-build loader picks it up directly from your source
directory (see [Discovery](#discovery-bundling-and-distribution));
for shipping, the `just build-plugin` / `just package-plugin`
recipes assemble the `.torchsnap` archive.

---

## The four guest traits

The SDK re-exports the four `wit_bindgen`-generated guest traits
flatly through `torchsnap_plugin_sdk::prelude` so plugin code only
ever needs `use torchsnap_plugin_sdk::prelude::*;`:

| Trait              | WIT export      | Methods                                           |
|--------------------|-----------------|---------------------------------------------------|
| `LifecycleGuest`   | `lifecycle`     | `enable`, `disable`, `on_setting_changed`         |
| `SearchGuest`      | `search`        | `entries`, `search`, `execute`                    |
| `MessagingGuest`   | `messaging`     | `handle_message`                                  |
| `TasksGuest`       | `tasks`         | `run_task`                                        |

All methods are **free associated functions on the implementing
type** (no `&self`). State is held in `thread_local!`,
`LazyLock`, or `OnceCell`.

---

## Lifecycle and instance management

ADR 0033 specifies the bridge model: the host **compiles**
every plugin's component once at launcher startup and caches the
resulting `wasmtime::Component`. **Instantiation** (linker setup,
`Store` creation, guest-side initial-state allocation) happens
lazily, on the first enable, and is repeated on every subsequent
re-enable after a disable.

### `enable()`

Called every time the plugin transitions from disabled to
enabled. The host has already created the per-plugin SQLite
database file and applied any declared migrations *before* this
runs, so `sql::connection()` is immediately usable. Typical
work:

- Read settings via `settings::get_or` / `get_or_else`.
- Seed in-memory caches from SQL, bundled assets, or HTTP.
- Update reactive state (`thread_local! { Cell<…> }`) from
  defaults to the user's configured values.

### `disable()`

Called when the user toggles the plugin off, and once on
application exit. The bridge drops the `Store` after this
returns, so any state held in linear memory is reclaimed
automatically — `disable()` only needs to flush anything
not already persisted (most plugins do nothing).

### `on_setting_changed(key, value)`

Called when a setting under `plugins.<plugin-id>.*` changes.
`value` is a JSON-encoded string, same encoding as
`settings::get`. Rapid same-key writes are coalesced by the
host's `CoalescingDispatcher` (ADR 0026) before this fires, so
each invocation already carries the latest value — no debouncing
needed on the plugin side.

---

## Search: catalog vs query, prefix routing, response variants

A plugin can serve any combination of three search modes:

| Mode          | Implements              | When to use                                      |
|---------------|-------------------------|--------------------------------------------------|
| Catalog       | `entries()`             | Static/semi-static lists; host fuzzy-matches.    |
| Query (always)| `search()`              | Dynamic results merged with all other plugins.   |
| Query (prefix)| `search()` + manifest `prefixes` | Exclusive routing when the query starts with the prefix. |

### `entries()`

Returns a `Vec<CatalogEntry>`. The host runs nucleo fuzzy
matching across `title` and `keywords` and merges the matches
with the rest of the active result list. Called on every
keystroke — keep it fast and side-effect free.

```rust
fn entries() -> Vec<CatalogEntry> {
    vec![CatalogEntry {
        id: "my-action".into(),
        title: "My Action".into(),
        subtitle: Some("Does the thing".into()),
        icon: Some(EntryIcon::HeroIcon("bolt".into())),
        keywords: vec!["thing".into()],
        actions: vec![Action {
            id: ActionId::Open,
            label: "Run".into(),
        }],
    }]
}
```

### `search(query, matched_prefix)`

Returns a `SearchResponse` variant (WIT lines 733–745):

| Variant                 | Effect                                                         |
|-------------------------|----------------------------------------------------------------|
| `Nothing`               | Plugin contributes nothing this keystroke.                     |
| `Results(Vec<ScoredEntry>)` | Pre-scored entries merged into the host's list.            |
| `CustomUi(ViewResponse)`| Host mounts the plugin's named view component, replacing the result list. |
| `InlineUi(ViewResponse)`| Host renders the plugin's inline view above the result list (only one plugin per search may claim the slot). |

`ViewResponse { view, data, results }`:

- `view` resolves against the manifest's `[frontend.views]` /
  `[frontend.inline-views]` map.
- `data` is `Option<String>` carrying a JSON-encoded payload
  (the WIT has no opaque JSON type — plugins serialize and
  deserialize with `serde_json`).
- `results` is forwarded to the React component as props; use
  it or ignore it depending on whether the view wants to
  render its own list.

### Prefix routing (ADR 0012)

If `manifest.toml` declares `plugin.prefixes = ["="]`, the host
strips the matched prefix and routes the query **exclusively**
to that plugin. Other catalog plugins and prefix-less query
plugins are bypassed for the keystroke. The `matched_prefix`
parameter receives the prefix string when this fires (`Some("=")`),
and `None` otherwise.

Longest prefix wins when multiple plugins register prefixes that
could both match.

### `ScoredEntry`

```rust
pub struct ScoredEntry {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    pub title_highlight_positions: Vec<u32>,    // UTF-16 code unit offsets
    pub subtitle_highlight_positions: Vec<u32>, // UTF-16 code unit offsets
    pub actions: Vec<Action>,
}
```

The plugin is responsible for scoring and for computing the
highlight offsets that the launcher renders as bold characters
in the title/subtitle. Frecency bonuses are applied **by the
host** after `search()` returns — plugins do not need to layer
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

`Action` (WIT 688–691) is `{ id: ActionId, label: String }`. The
ordered `actions` vector on a `CatalogEntry` / `ScoredEntry`
puts the primary (Enter) action first; subsequent entries appear
in the action palette (Cmd+K).

`ActionId` (WIT 678–686):

```rust
pub enum ActionId {
    Open,
    Copy,
    Reveal,
    OpenWith,
    Delete,
    OpenSettings,           // Jumps to this plugin's settings panel.
    Custom(String),         // Plugin-defined action.
}
```

`Open`, `Copy`, etc. are well-known so the launcher can render
default icons and key hints. `Custom("save-as-bookmark")` covers
plugin-specific actions.

`execute()` is called when the user activates an action. Returns
`Result<PostAction, String>`:

```rust
pub enum PostAction { Nothing, Dismiss, KeepOpen }
```

There is no `ShowCustomUI` variant — custom UI is mounted only
through `SearchResponse::CustomUi` from `search()`.

```rust
fn execute(entry_id: String, action_id: ActionId) -> Result<PostAction, String> {
    match (entry_id.as_str(), &action_id) {
        ("open-docs", ActionId::Open) => {
            opener::open_url("https://example.com/docs")
                .map_err(|e| format!("open url: {e:?}"))?;
            Ok(PostAction::Dismiss)
        }
        _ => Err(format!("unhandled: {entry_id} / {action_id:?}")),
    }
}
```

Returning `Err(string)` surfaces in the host log and as a toast
in the launcher; the user-visible error message is the returned
string verbatim.

---

## Messaging (frontend ↔ plugin RPC)

The WIT `messaging::handle-message` export (lines 790–793) is the
single entry point a plugin's frontend component calls into when
it needs backend work done. The SDK helpers in
`plugins/plugin-sdk/src/messaging.rs` cover the JSON encode /
decode boilerplate.

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct LookupRequest { domain: String }

#[derive(Serialize)]
struct LookupResponse { title: Option<String> }

impl MessagingGuest for MyPlugin {
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            "lookup" => {
                let req: LookupRequest = messaging::parse_payload(&payload)?;
                let title = lookup_title(&req.domain);
                messaging::to_response(&LookupResponse { title })
            }
            other => Err(format!("unknown method: {other}")),
        }
    }
}
```

**WASM plugins cannot stream.** The export is strictly request /
response. The TypeScript `sendMessage(method, payload, onMessage)`
overload exists for symmetry with native plugins but
WASM-backed plugins drop the channel callback silently — see ADR
0030 and `packages/plugin-sdk/src/shims/hooks.ts:88` for the
rationale and the future `messaging-stream` sub-interface that
would lift the restriction.

---

## Scheduled tasks

WASM has no thread support, so plugins cannot run their own
timers. Instead, declare `[[tasks]]` entries in `manifest.toml`
and implement `TasksGuest::run_task` to dispatch by `task_id`:

```toml
[[tasks]]
id = "retention-cleanup"
schedule = "*/30 * * * *"
```

```rust
impl TasksGuest for MyPlugin {
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
supported. The host runs a per-plugin tokio scheduler that walks
every entry, sleeps until the earliest next fire, and invokes
`run-task` on the configured cadence (ADR 0032).

Semantics:

- **First fire is at the next cron match.** No automatic
  immediate fire on enable. Plugins that want startup work
  should do it in `enable()`.
- **Failures are logged, not fatal.** Returning `Err(string)`
  is logged at error level and does not auto-disable the
  plugin.
- **Missed fires across launcher restarts are not made up.**
- **Sequential execution.** Two tasks scheduled at the same
  instant run back-to-back in manifest order; the wasmtime
  store mutex serializes every guest call.
- **Plugins without `[[tasks]]` pay zero cost** — the host
  never spawns the scheduler loop. `impl_noop_tasks!(MyPlugin);`
  is enough to satisfy the WIT contract.

---

## Manifest (`manifest.toml`)

The full schema is in `src-tauri/src/wasm/manifest.rs`. Every
table is documented inline there.

### `[plugin]` (required)

```toml
[plugin]
id          = "myplugin"            # lowercase a-z 0-9 hyphens; no leading/trailing -
name        = "My Plugin"
description = "Short one-liner"
version     = "0.1.0"
wasm        = "myplugin_plugin.wasm" # plugin-relative path to the component
icon        = "heroicons:bolt"       # or a plugin-relative path to a WebP image
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

Per-plugin SQLite database. The host creates the database file
and applies migrations *before* the guest's `enable()` runs.

```toml
[storage.sql]
migrations = [
    "migrations/001_init.sql",
    "migrations/002_add_index.sql",
]
```

Each path is plugin-relative. Files are plain SQL — diffable,
shareable with unit tests via `include_str!`, and `query_one` /
`query_all` helpers in the SDK consume the host's response rows.

### `[[tasks]]`

Repeat-table form for scheduled background tasks. Fields:

- `id` — must be unique within the manifest; passed back to
  `run_task`.
- `schedule` — 5-field POSIX cron expression.

### `[frontend]`

Frontend bundle declarations. Omit entirely if the plugin has no
UI. All paths are plugin-relative.

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
is omitted entirely means the plugin has no access to that
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

[permissions.fs]
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
`manifest.rs:570–610` and ADR 0040. `${plugin-data}`,
`${plugin-archive}`, `${home}`, `${xdg-config}`, `${xdg-data}`
substitution applies in both `[permissions.fs] read` patterns
and `[[permissions.command]]` `path-under` roots / per-rule
`cwd`.

### `[shortcuts]` (declarative only — currently inert for WASM)

`manifest.rs:54` accepts a `[shortcuts]` table but the WIT does
not currently expose a `handle-shortcut` guest export, so any
declared shortcuts are parsed and stored but never fire for
WASM plugins. Native (built-in) plugins still use them. Treat
this section as reserved for a future API; do not plan around
it.

### Path-safety rules

Every plugin-relative path the manifest references (`wasm`,
asset `icon`, `frontend.*-bundle`, `frontend.*-css`, every
migration path) must satisfy `validate_plugin_path`:

- No `..` segments.
- No leading `/`.
- No backslashes.
- No Windows drive letters.
- No NUL bytes.

A manifest that violates any of these fails to parse and the
plugin never loads.

---

## Host APIs (WIT imports)

The WIT `plugin` world (`torchsnap-plugin.wit:887–907`) imports
fourteen interfaces. The SDK re-exports each one — either flat
under `torchsnap_plugin_sdk::prelude::*` (`logging`, `clipboard`,
`sql`, `frecency`, `settings`, `opener`, `http`, `fs`, `assets`,
`command`, `platform`, `paths`, `messaging`, `website_metadata`)
or under a `_host` alias when the SDK adds an ergonomic wrapper
that would otherwise shadow the raw bindings.

### `logging`

Structured logging with optional spans. The SDK's
`plugins/plugin-sdk/src/logging.rs` provides `log_info!`,
`log_warn!`, `log_error!`, `log_debug!`, `log_trace!` macros
modelled on `tracing`:

```rust
log_info!("user toggled feature", "feature" => "history", "enabled" => true);
```

For explicit timing spans, call `logging::span_start` /
`logging::span_end` directly.

### `clipboard`

Single operation: `clipboard::write_text(text) -> Result<(), String>`.
Read access is intentionally not exposed — sandboxed plugins must
not be able to slurp arbitrary host clipboard contents (WIT
lines 35–55).

### `sql` — per-plugin SQLite

Available only when the manifest declares `[storage.sql]`.
`sql::connection()` is **infallible** for plugins that opted in
(the host has already initialized the database). The SDK's
`sql::query_one` / `sql::query_all` collect rows into a typed
`Row` newtype with `integer` / `text` / `real` / `blob` /
`is_null` accessors. `From` impls on `SqlValue` cover the common
parameter types:

```rust
use torchsnap_plugin_sdk::sql::{query_all, SqlValue};

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
internal `List` variant — plugins build their own `IN (?, ?, ?)`
clauses (one `?` per element) before calling `query` / `execute`.
Transactions are not currently exposed; raise the API gap if a
plugin needs them.

> The WIT doc-comment on `interface sql` (`torchsnap-plugin.wit:60-61`)
> states the database file lives at
> `app_data_dir/plugins/<plugin-id>/storage.db`. That comment is
> stale. The actual path used by the bridge is
> `<app_data_dir>/plugin-home/<plugin-id>/sql/storage.sqlite3`
> (see [Storage layout on disk](#storage-layout-on-disk)).

### `frecency`

Read-only. The host already records selections automatically
before dispatching `execute()` and applies score bonuses to
`search()` results — neither requires plugin code. The interface
exposes only:

```rust
frecency::is_enabled() -> bool
frecency::top_items(limit: u32) -> Vec<FrecencyItem>
```

Use `top_items` to drive empty-query "most-used first" ordering
in custom UIs (e.g. the emoji picker's browse mode). There is
**no `record` operation** in the WIT — plugins cannot push
synthetic frecency events from custom UIs.

### `settings`

Plugin-namespaced reads only. The raw WIT (`settings::get(key) -> Option<String>`)
returns JSON-encoded values; the SDK's
`plugins/plugin-sdk/src/settings.rs` wraps it:

```rust
let verbose: bool = settings::get_or("verbose", false);
let greeting: String = settings::get_or_else("greeting", || "(unset)".into());
```

Writes from the plugin side are deliberately not exposed — the
launcher's settings UI is the single source of mutation. The
plugin reacts to changes through `LifecycleGuest::on_setting_changed`.

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
use torchsnap_plugin_sdk::http::{HttpMethod, HttpRequest};

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
the plugin can reach.

### `fs`

Read-only filesystem access through a manifest-declared
allowlist:

```toml
[permissions.fs]
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

Read files from inside the plugin's own `.torchsnap` archive (or
development directory). **No manifest opt-in** — every plugin
can read its own bundled files. Spatial isolation is enforced
by `validate_plugin_path`.

```rust
use torchsnap_plugin_sdk::assets::AssetsError;

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
cross the WIT boundary in full on every `read`; plugins that
need to keep an asset around should hold one copy in their own
linear memory after the first read.

### `command`

Run a system process. Plugins declare repeated
`[[permissions.command]]` rules; each `run` call must match
exactly one rule. The SDK's
`plugins/plugin-sdk/src/command.rs` provides a builder:

```rust
use std::time::Duration;
use torchsnap_plugin_sdk::command;

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
start`) are rejected at manifest parse time — plugins that want
"open with the registered application" use
`[permissions.opener] open-path = true` instead.

### `platform`

Runtime OS / arch detection. WASI does not expose either to
guests, and plugins making platform-conditional decisions
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
let helper = paths::resolve("${plugin-archive}/bin/helper")?;
let result = command::run(&helper).arg("status").invoke()?;
```

Recognized variables: `plugin-data`, `plugin-archive`, `home`,
`xdg-config`, `xdg-data`.

### `website-metadata`

Shared host cache for per-domain title / description /
favicon. Gate with `[permissions]\nwebsite-metadata = true`.
The SDK's `plugins/plugin-sdk/src/website_metadata.rs` flattens
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

Plugin frontends are React components bundled with Vite and
loaded into the launcher / settings webviews via dynamic
`import()` over the host-side `torchsnap-plugin://` URL scheme
(ADR 0028).

### `@torchsnap/plugin-sdk` package

Plugins declare a `file:`-protocol dependency on the in-tree
SDK package at `packages/plugin-sdk/`. Subpath exports
(`packages/plugin-sdk/package.json`):

| Subpath                            | What it exports                                                |
|------------------------------------|----------------------------------------------------------------|
| `@torchsnap/plugin-sdk`            | Type-only: `PluginViewProps`, `InlineViewProps`, `PluginSettingsProps`, `ActionId`, `EntryIcon`, `SourcedEntry`, `Action`, `Logger`. |
| `@torchsnap/plugin-sdk/hooks`      | The four host hooks (see below).                               |
| `@torchsnap/plugin-sdk/components` | Shared launcher building blocks.                               |
| `@torchsnap/plugin-sdk/keybindings`| Keybinding helper utilities.                                   |
| `@torchsnap/plugin-sdk/utils`      | Misc helpers.                                                  |
| `@torchsnap/plugin-sdk/testing`    | Test-only mocks.                                               |
| `@torchsnap/plugin-sdk/vite`       | The `torchsnap()` Vite plugin.                                 |
| `@torchsnap/plugin-sdk/theme.css`  | Shared design-token CSS.                                       |

### `vite.config.ts`

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { torchsnap } from "@torchsnap/plugin-sdk/vite";

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
(`packages/plugin-sdk/src/vite/index.ts`) aliases `react` and
`react/jsx-runtime` to SDK shims that read the host-supplied
React off `window.__torchsnap.React`. Plugin bundles ship only
import declarations — the React runtime stays in the host.

### Component contract (ADR 0028)

The host wraps every plugin mount in a `<PluginContextProvider>`
that supplies identity, runtime capabilities, launcher actions,
and reactive setting accessors. Plugin components receive
**only** per-render data through their props; everything else
flows through hooks. Sub-components extracted from a plugin
component can call hooks directly — no prop threading required.

The four hooks (`packages/plugin-sdk/src/shims/hooks.ts`):

```tsx
import {
  usePluginInfo,
  usePluginRuntime,
  useLauncher,
  usePluginSetting,
} from "@torchsnap/plugin-sdk/hooks";

import type { PluginViewProps } from "@torchsnap/plugin-sdk";

export function MyView({ data, results, query, matchedPrefix }: PluginViewProps) {
  const { id, enabled } = usePluginInfo();              // identity (always)
  const { sendMessage, logger } = usePluginRuntime();   // RPC + logging (always)
  const { dismiss, onExecute, onFooterChange } = useLauncher(); // launcher tree only
  const [retentionDays, setRetentionDays] = usePluginSetting<number>("retentionDays");
  // ...
}
```

`useLauncher` throws when called from a settings panel; the
other three are safe everywhere.

### Per-render props

| Prop kind            | Carries                                                       |
|----------------------|---------------------------------------------------------------|
| `PluginViewProps`    | `results`, `data`, `query`, `matchedPrefix`                   |
| `InlineViewProps`    | `data`, `query`, `matchedPrefix`, `selected`                  |
| `PluginSettingsProps`| (empty — every value comes from hooks)                        |

`data` is whatever the plugin's `search()` set on
`ViewResponse.data` (a JSON-encoded string the host decodes
before passing). The plugin defines its own request / response
shapes.

### `sendMessage` is one-shot for WASM plugins

```ts
const result = await sendMessage<Req, Resp>("lookup", { domain: "example.com" });
```

The `onMessage` streaming callback is silently dropped by the
WASM bridge (`packages/plugin-sdk/src/shims/hooks.ts:88`).

### Icons (ADR 0027)

`EntryIcon` is a string-tagged enum on the WIT side. The
launcher resolves `HeroIcon("bolt")` against the embedded
Heroicons set; `DataUrl` and `AssetIcon` render directly;
`Emoji` renders as glyph text.

---

## Storage layout on disk

Per-plugin host-managed state lives under
`<app_data_dir>/plugin-home/<plugin-id>/`, separate from
plugin **code** which lives under
`<app_data_dir>/plugins/`. SQLite databases use the
`.sqlite3` extension project-wide.

| Path                                                              | Owner           |
|-------------------------------------------------------------------|-----------------|
| `<app_data_dir>/plugins/<id>.torchsnap`                           | User-installed plugin archives. |
| `<app_data_dir>/plugin-home/<id>/sql/storage.sqlite3`             | Per-plugin SQLite database. |
| `<app_data_dir>/plugin-home/<id>/exec-cwd/`                       | Default cwd for `command::run` calls. |
| `<resource_dir>/plugins/`                                         | App-bundle plugins (read-only). |

Uninstalling a user plugin removes both `plugins/<id>.torchsnap`
and `plugin-home/<id>/`. See ADR 0035 (distribution) and
ADR 0018 (SQL storage).

---

## Discovery, bundling, and distribution

### Discovery roots (ADR 0035)

The host scans three roots at startup, in precedence order:

1. **System** — `<resource_dir>/plugins/`. Shipped inside the
   app bundle; populated at build time from
   `plugins/bundled.toml`.
2. **Dev** — `<CARGO_MANIFEST_DIR>/../plugins/`. Debug builds
   only; release builds never touch this root.
3. **User** — `<app_data_dir>/plugins/`. Where user-installed
   `.torchsnap` archives land.

A plugin id can only be registered once; the first root to
claim a given id wins. Inside a single root, an archive
(`foo.torchsnap`) wins over a sibling directory (`foo/`).

The host tags each loaded plugin with a `PluginSourceKind`
(`system`, `dev`, `user`, `builtin`); the Plugins settings
panel renders this as a badge and gates the uninstall action
to `user` only.

### Shipping with the app

The release bundle includes only plugins listed in
`plugins/bundled.toml`. To add one:

1. Add the plugin id (a directory name under `plugins/`) to the
   list.
2. Run `just build`.

The `stage-bundled-plugins` recipe rebuilds each listed plugin,
packages it as `.torchsnap`, copies it into
`target/bundled-plugins/`, and Tauri picks it up via the
`resources` entry in `tauri.conf.json`.

The repo-root `target/` directory is gitignored and owned
entirely by this staging flow. Cargo itself uses
`src-tauri/target/` (host) and `plugins/target/` (plugin
workspace) — those are unrelated.

### Sharing a plugin with users

Send users the `.torchsnap` file. They drop it onto the
Plugins settings panel (file picker or drag-drop); the app
copies it to `<app_data_dir>/plugins/<id>.torchsnap` and
prompts for a restart.

### Trust model (ADR 0036)

There is no archive signing today. The sandbox is the WIT
capability surface: no ambient network access, read-only
filesystem through a manifest allowlist, write-only clipboard,
no thread spawning, no arbitrary process exec (only
manifest-declared rules). Users trust the source they got the
archive from; the host limits what the plugin can reach beyond
that. ADR 0036 documents the rationale and the triggers for
revisiting.

---

## Worked example: the `calculator` plugin

`plugins/calculator/` exercises the full plugin authoring
surface in one place: prefix routing, custom UI, inline UI,
SQL persistence, scheduled retention, and reactive settings.

### Manifest

```toml
# plugins/calculator/manifest.toml
[plugin]
id          = "calculator"
name        = "Calculator"
description = "Evaluate math expressions with history tracking"
version     = "0.1.0"
wasm        = "calculator_plugin.wasm"
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

### Plugin code (highlights)

State lives in `thread_local!` `Cell`s — `wasm32-wasip2` is
single-threaded, so plain interior mutability is sufficient.
`enable()` seeds them from settings; `on_setting_changed`
refreshes them when the user toggles the setting:

```rust
// plugins/calculator/src/lib.rs (excerpt)

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
fn search(query: String, matched_prefix: Option<String>) -> SearchResponse {
    if matched_prefix.is_some() {
        let payload = json!({ "expression": query, "result": evaluate(&query) });
        return SearchResponse::CustomUi(ViewResponse {
            view:    "history".into(),
            data:    Some(payload.to_string()),
            results: vec![],
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

The frontend uses `usePluginRuntime().sendMessage("save",
{ … })` to persist new history rows, `usePluginSetting<number>("retentionDays")`
to render the settings slider, and `useLauncher().onExecute` to
trigger the standard "copy result" action through the host's
action palette. The Rust `MessagingGuest::handle_message`
implementation dispatches `"save" | "load" | "delete"` against
the SQL store.

The full source is at `plugins/calculator/src/lib.rs`; reading
it end-to-end is the fastest way to see every piece in
context.
