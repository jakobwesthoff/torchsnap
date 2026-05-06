# Self-Contained Gadget System — Strategy Document

> Living document. Updated as design decisions are made during discussion.

**Testing policy:** All new code in the WASM gadget system must have extensive
test coverage. Every struct, every validation path, every error case. This is
foundational infrastructure — correctness matters more than velocity.

## 1. Vision

Gadgets become self-contained modules — `.torchsnap` zip archives containing a
WASM backend component and pre-bundled frontend assets. They are discovered,
loaded, and activated at runtime without recompilation of the host. The existing
native gadget system runs in parallel during migration and is removed once all
gadgets have been converted.

## 2. Prior Research

Decision already made in prior research (now archived):

- **WIT + Component Model** via `wasmtime` (not Extism, not raw wasip1)
- Compile-time type safety across host/guest boundary via `wasmtime::component::bindgen!`
- Gadget authors write Rust (most mature), Go, or potentially JS/Python
- The existing `Gadget` trait comment already notes it was "structured so it can
  later become the boundary for a WASM gadget interface"

## 3. Current System Inventory

### 3.1 Gadget Trait (historical, pre-WASM)

> **Historical.** This subsection captures the native `Gadget` trait
> as it existed when the WASM plan was drafted. The trait still exists
> in `src-tauri/src/gadgets/mod.rs` for the few host-internal gadgets
> that stay native (see §11.4), but the `&self` / `tauri::AppHandle` /
> `ResultChannel` shape has been refactored: `search()` is call-return
> (no streaming), lifecycle is `enable/disable/setting_changed`, and
> WASM gadgets reach the host exclusively through the WIT contract in
> `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`.

| Method | WASM-relevant notes |
|---|---|
| `id()` | Trivial string export |
| `entries()` | Returns `Vec<CatalogEntry>` — serializable, crosses boundary cleanly |
| `search(query, prefix, results, cancel)` | Migrated to call-return `search() -> search-response`; no `ResultChannel` |
| `execute(entry_id, action_id, app)` | `AppHandle` replaced by WIT host imports (`clipboard`, `opener`, `http`, …) |
| `handle_message(method, payload, channel)` | WASM gets request/response only via `messaging::handle-message` (no streaming) |
| `setup(app, ctx)` | Replaced by `lifecycle::enable` plus host imports (`settings`, `frecency`, `sql`, …) |
| `initialize_settings(settings)` | Declarative `[settings]` table in `manifest.toml` |
| `shortcuts()` | Currently host-managed only for native gadgets; WASM exposes no shortcut export yet |
| `handle_shortcut(id, app)` | Not part of the WIT contract — open question |
| `teardown()` | Replaced by `lifecycle::disable` |

### 3.2 Host Capabilities Used by Gadgets

Analysis of what each gadget actually needs from the host. The
right-most column tracks current state against the WIT contract in
`gadgets/gadget-sdk/wit/torchsnap-gadget.wit` — the candidate
interface name (which informed the as-built name) and the status:

| Capability | Used by | WIT interface | Status |
|---|---|---|---|
| Logging + spans | all | `logging` | Done |
| SQL storage | calculator, bangs, clipboard | `sql` | Done (ADR 0031) |
| Per-gadget asset reads | template, bangs, emoji-picker | `assets` | Done (ADR 0039) |
| Clipboard write | calculator, emoji, bangs, open-url, clipboard | `clipboard` | Done (write-only; read deliberately omitted) |
| Open URL / path / reveal | bangs, open-url, zerotier | `opener` | Done (ADR 0037) |
| HTTP fetch | bangs, zerotier | `http` | Done (ADR 0038) |
| Filesystem reads (manifest-gated) | zerotier, future gadgets | `fs` | Done |
| Subprocess execution (manifest-gated) | zerotier, future gadgets | `command` | Done (ADR 0040) |
| Platform/arch detection | zerotier | `platform` | Done |
| Manifest path resolution | zerotier | `paths` | Done |
| Website metadata (title, favicon) | bangs, open-url | `website-metadata` | Done |
| Settings read | all with settings | `settings` | Done (ADR 0029) |
| Settings reactivity | calculator, clipboard, bangs | `lifecycle::on-setting-changed` | Done (callback, ADR 0026/0029) |
| Frecency read | emoji-picker | `frecency` | Done (read-only — host records writes automatically) |
| Scheduled background tasks | clipboard retention, future | `tasks` (cron) | Done (ADR 0032) |
| Custom RPC frontend↔gadget | clipboard, calculator UI | `messaging::handle-message` | Done (request/response only, ADR 0030) |
| Blob (large binary) storage | clipboard | `storage/blob` | Not yet — sibling dirs under `gadget-home/<id>/` planned (ADR 0018/0035) |
| App discovery + launch | app-launcher | — | Stays native (see §11.4) |
| System preferences discovery | system-preferences | — | Stays native |
| App exit / show settings window | `commands` built-in | — | Stays native |

### 3.3 Frontend Gadget Surface

Each gadget can provide:

- **Views** (`Record<string, ComponentType<GadgetViewProps>>`) — custom UI replacing result list
- **Inline views** (`Record<string, ComponentType<InlineViewProps>>`) — widget above result list
- **Settings component** (`ComponentType<GadgetSettingsProps>`) — settings panel

Components must be ES modules with `default` exports. They consume a stable
props interface and communicate with the backend via `sendMessage` (already
JSON-based, natural fit for WASM).

The registry (`src/gadgets/registry.ts`) is explicitly marked as a TODO for
dynamic resolution. The `gadgetComponent.tsx` lazy-loading infrastructure
already supports dynamic `import()` factories.

### 3.4 Build System

- Vite 8 (rolldown-based) with manual chunk splitting that already isolates
  gadget code from shared code
- `@torchsnap/keybindings`, `@torchsnap/components`, `@torchsnap/types` path
  aliases form the implicit "gadget frontend SDK"
- Tauri's `assetProtocol` scope already includes `$APPDATA/**` — usable for
  loading gadget assets from disk

## 4. Gadget Archive Format

### 4.1 `.torchsnap` Zip Archive

File extension: **`.torchsnap`** (alternative considered: `.torch`).

The WASM binary filename is declared in the manifest — not hardcoded. This
allows gadget authors to use meaningful names and avoids conflicts in multi-crate
workspaces.

```
emoji-picker.torchsnap (zip)
├── manifest.toml           # Gadget metadata + declarations
├── emoji_picker.wasm       # WASM component (path declared in manifest)
├── frontend/
│   ├── launcher.js         # ES module: views + inline views (loaded in launcher webview)
│   └── settings.js         # ES module: settings component (loaded in settings webview)
└── assets/                 # Optional static assets (icons, data files)
    └── icon.svg
```

### 4.2 `manifest.toml`

```toml
[gadget]
id = "clipboard-manager"                # Stable identifier (lowercase, alphanumeric + hyphens)
name = "Clipboard Manager"              # Human-readable display name
description = "Clipboard history with search and paste"  # Shown in settings section header
version = "0.1.0"                       # Gadget's own version (display, update checks)
wasm = "clipboard_manager.wasm"         # Path to WASM component within the archive
icon = "heroicons:clipboard-document-list"  # See icon format below
prefixes = [":"]                            # Optional — omit for catalog/always-on

[settings]
retentionDays = 90                      # Flat key-value defaults — the section IS the defaults map
bringToFrontOnPaste = true

[shortcuts]
open-clipboard = { label = "Open Clipboard History", default = "CmdOrCtrl+Shift+V" }

[frontend]
launcher-bundle = "frontend/launcher.js"  # ES module for the launcher webview
settings-bundle = "frontend/settings.js"  # ES module for the settings webview

[frontend.views]
history = "ClipboardView"                 # Named export from launcher-bundle

# [frontend.inline-views]
# result = "SomeInlineComponent"          # Named export from launcher-bundle

[frontend.settings]
component = "ClipboardSettings"           # Named export from settings-bundle
```

#### Design Decisions

**All `[gadget]` fields are required.** `id`, `name`, `description`, `version`,
`wasm`, and `icon` must all be present. The host uses `name`, `description`, and
`icon` to render the settings section header (`SectionHeader` component) —
without them, the gadget has no settings presence.

**Icon format** supports two modes via prefix:
- `"heroicons:<name>"` — resolved to a HeroIcon component (e.g.,
  `"heroicons:clipboard-document-list"`)
- Any other string — treated as a file path within the gadget archive/directory
  pointing to a WebP image (e.g., `"assets/icon.webp"`)

**Gadget ID is explicit, not derived from name.** The ID is the stable key for
settings namespaces (`gadgets.<id>.*`), frecency storage, data directories, and
frontend registry lookups. Deriving it from `name` would break user data if the
gadget is renamed. Authors pick an ID once; the host validates format
(lowercase, alphanumeric + hyphens only) at load time.

**No search `mode` field.** All WASM gadgets implement the full guest export
surface (`entries()`, `search()`, `execute()`, etc.). A catalog-only gadget
returns an empty list from `search()`; a query-only gadget returns an empty list
from `entries()`. This mirrors the native `Gadget` trait where all methods have
no-op/empty defaults. The host always calls both paths — the cost of an empty
return is negligible.

**No `api-version` yet.** Deferred until the 1.0.0 release when API stability
matters. During development the WIT contract changes freely.

**Enable/disable is a host-level concern.** There is no `enabled` key in
`[settings]`. Every gadget is enable/disable-able, managed entirely by the host.
The host:
- Stores the enabled state in its own storage (not the gadget's settings
  namespace)
- Renders the enable/disable toggle in the settings UI for every gadget
- Never calls `search`/`entries`/`execute` on disabled gadgets
- **Does not instantiate the WASM module at all for disabled gadgets.** Only the
  manifest is parsed (cheap). No `wasmtime::Engine`, no `Store`, no memory.

**No separate `on-enable`/`on-disable` exports.** Since the host drops and
re-instantiates the entire WASM module on enable/disable, `setup()` *is* the
enable handler and `teardown()` *is* the disable handler. There's no persistent
WASM state between disable→re-enable — the gadget starts fresh every time.

Lifecycle:
```
Discovered (manifest parsed only) → disabled
    ↓ user enables
Instantiate WASM → enable() → active
    ↓ user disables
disable() → drop WASM instance → discovered
    ↓ user re-enables
Instantiate WASM → enable() → active (fresh start)
```

> **Implementation note (2026-04-05):** The actual implementation diverged
> here — `enable()` and `disable()` are explicit WIT guest exports in
> `torchsnap-gadget.wit`, and the WASM instance is currently kept alive
> across toggles rather than dropped and re-instantiated. This is more
> flexible: gadgets can do expensive one-time setup in `enable()` without
> paying re-instantiation cost on every toggle. The host side uses
> `GadgetSlot` (with `AtomicBool` + `CoalescingDispatcher`) in
> `gadget_host.rs` to manage the enabled state and dispatch
> `setting_changed()` calls.
>
> **Open question:** We may still move to destroying WASM instances on
> disable (matching the original design) for memory savings. The
> `enable()`/`disable()` guest exports would remain regardless — they
> serve as lifecycle hooks whether the instance is long-lived or
> re-created on each toggle.

The `[gadget]` section provides `name`, `description`, and `icon` so the host
can render the settings sidebar entry without any frontend code from the gadget.
Every gadget gets a settings entry even if it has no custom settings — the
enable/disable toggle is always present.

**Shortcuts are host-managed.** The manifest declares shortcuts (ID, label,
default keybinding), but the actual keybinding value is stored and managed by
the host. The gadget receives `handle-shortcut(id)` when a shortcut fires —
it never sees or manages the keybinding itself. The settings UI renders shortcut
configuration below the gadget's custom settings section or in a global
shortcuts area (TBD).

**Settings are flat defaults.** The `[settings]` section is a direct key-value
map of default values. No nesting, no `defaults` sub-key. The host applies
these as defaults (only filling missing keys, never overwriting user values) at
gadget load time. Settings migrations (rename/remove keys) will be handled
through a separate mechanism if needed.

**Two frontend bundles (launcher + settings).** Torchsnap uses two separate
webviews — the launcher window and the settings window. Loading a single bundle
in both would pull unnecessary code into each window and risk cross-window side
effects (the exact problem the current `manualChunks` Vite config solves). So
each gadget produces up to two ES module bundles:
- `launcher-bundle` — contains views and inline views (loaded in the launcher)
- `settings-bundle` — contains the settings component (loaded in settings window)

Either can be omitted if the gadget has no components for that window. Named
exports map component names from each bundle.

### 4.4 Gadget Source Abstraction

During development, requiring a `.torchsnap` zip on every change is impractical.
The loader uses a source abstraction:

```
trait GadgetSource
├── DirectorySource  — reads manifest.toml + files from a folder on disk
└── ArchiveSource    — reads from a .torchsnap zip
```

Both yield the same `Manifest` struct and provide access to the WASM binary and
frontend assets. The `GadgetHost` (and `WasmGadget` adapter) are agnostic to
which source loaded the gadget.

> **Superseded by ADR 0035.** The discovery and layout details below
> have been finalized as three precedence-ordered search roots
> (`<resource_dir>/gadgets/` for bundled System gadgets,
> `<CARGO_MANIFEST_DIR>/../gadgets/` for Dev gadgets in debug builds,
> `<app_data_dir>/gadgets/` for user-installed gadgets). See ADR 0035
> for the authoritative rules; this paragraph is preserved for
> historical design context.

**Development workflow:** the debug-build loader automatically scans
`<CARGO_MANIFEST_DIR>/../gadgets/` — no config or CLI flag needed.
**Production:** `<resource_dir>/gadgets/` for bundled `.torchsnap`
archives and `<app_data_dir>/gadgets/` for user-installed ones.

### 4.3 Why Zip?

- Well-understood format with excellent Rust support (`zip` crate)
- Single-file distribution — no directory structure to manage
- Can be read in-memory without extracting to disk (though extraction to a
  cache dir is fine for frontend assets that the WebView needs to load via URL)
- The frontend JS files need to be accessible via a URL for dynamic `import()`.
  The ADR-0035 install flow keeps archives intact on disk; frontend
  assets are served via a custom `torchsnap-gadget://` protocol
  handler that streams them straight out of the zip.

## 5. WIT Interface Design

### 5.1 Layered Interface Architecture

Rather than one monolithic `gadget` world, split host capabilities into
independent WIT interfaces that can evolve separately:

```
torchsnap:gadget@0.1.0
├── world gadget               # The main world gadgets target
│   ├── import torchsnap:core/lifecycle
│   ├── import torchsnap:core/search
│   ├── import torchsnap:core/settings
│   ├── import torchsnap:host/storage
│   ├── import torchsnap:host/clipboard
│   ├── import torchsnap:host/opener
│   ├── import torchsnap:host/http
│   ├── import torchsnap:host/frecency
│   ├── import torchsnap:host/logging
│   ├── export torchsnap:core/lifecycle  (guest side)
│   └── export torchsnap:core/search     (guest side)
```

This lets us:
- Add new host capabilities (new interfaces) without breaking existing gadgets
- Version interfaces independently
- Potentially gate capabilities per-gadget (sandboxing)

### 5.2 Core Guest Exports (what gadgets implement)

```wit
// torchsnap:core/lifecycle
interface lifecycle {
    enable: func();
    disable: func();
    on-setting-changed: func(key: string, value: string);
}

// torchsnap:core/search
interface search {
    use torchsnap:core/types.{catalog-entry, scored-entry, action-id, post-action, search-response};

    // Catalog mode
    entries: func() -> list<catalog-entry>;

    // Query mode — pure call-return, no streaming callback
    search: func(query: string, matched-prefix: option<string>) -> search-response;

    // Execution
    execute: func(entry-id: string, action-id: action-id) -> result<post-action, string>;
    handle-shortcut: func(shortcut-id: string) -> result<post-action, string>;
    handle-message: func(method: string, payload: string) -> result<string, string>;
}
```

### 5.3 Host Imports (what the host provides)

> **Sketch only.** The shapes below were the original strategy
> sketches. The authoritative contract lives in
> `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` and differs in
> several places:
>
> - `sql` uses a typed `sql-value` variant (null/integer/real/text/blob)
>   plus a `sql-handle` resource — not stringly-typed JSON. Gadgets call
>   `sql::connection()` once; the host pre-creates the database file
>   under `<app_data_dir>/gadget-home/<id>/sql/storage.sqlite3` and
>   applies migrations declared in `[storage.sql]` before `enable()`
>   runs (ADR 0031).
> - `frecency` is **read-only** — `is-enabled()` and `top-items(limit)`.
>   The host records selections automatically before dispatching
>   `execute()`, so gadgets never call `record()`. Score-bonus
>   application happens host-side after `search()` returns.
> - `opener` exposes `open-url`, `open-path`, and `reveal-path`, gated
>   by per-capability declarations under `[permissions.opener]`
>   (ADR 0037).
> - `http` is `fetch(http-request) -> result<http-response, http-error>`
>   with method/headers/body/timeout/max-body-size/insecure-tls,
>   gated by `[permissions.http] origins = […]` (ADR 0038).
> - `logging::log` carries structured metadata and an optional span
>   handle; spans are opened/closed via `span-start`/`span-end`.
> - `settings::get` takes a key relative to the gadget's namespace and
>   returns a JSON-encoded string (ADR 0029). Reactivity is a guest
>   export (`lifecycle::on-setting-changed`), not a host import.
>
> The original sketches are retained below for design context.

```wit
// torchsnap:host/storage
interface storage {
    type db-handle = u32;

    open: func(name: string) -> result<db-handle, string>;
    execute: func(db: db-handle, sql: string, params: list<string>) -> result<u64, string>;
    query: func(db: db-handle, sql: string, params: list<string>) -> result<string, string>;
    // query returns JSON rows — avoids complex WIT record for arbitrary schemas
}

// torchsnap:host/clipboard
interface clipboard {
    write-text: func(text: string) -> result<_, string>;
    // read-text intentionally omitted (privacy footgun)
}

// torchsnap:host/opener
interface opener {
    open-url: func(url: string) -> result<_, string>;
}

// torchsnap:host/http
interface http {
    record http-response {
        status: u16,
        body: list<u8>,
    }
    get: func(url: string) -> result<http-response, string>;
    // post: func(url: string, body: list<u8>) -> result<http-response, string>;  // future
}

// torchsnap:core/settings
interface settings {
    get: func(key: string) -> option<string>;    // JSON-encoded value
    // watch is harder — see open question below
}

// torchsnap:host/frecency (sketch — actual interface is read-only)
interface frecency {
    score: func(item-id: string) -> u32;
    top-items: func(limit: u32) -> list<frecency-item>;

    record frecency-item {
        item-id: string,
        score: u32,
    }
}

// NOTE: No `results` callback interface. See §5.7.

// torchsnap:host/logging
interface logging {
    enum log-level { trace, debug, info, warn, error }
    log: func(level: log-level, message: string);
}
```

### 5.4 WASI Imports and Sandboxing

Building with `--target wasm32-wasip2` causes the Rust standard library to
pull in WASI Preview 2 imports (`wasi:cli`, `wasi:io`, etc.). These appear
in the component's import list but **do not grant capabilities by default**.

The host creates a `WasiCtx` via `WasiCtxBuilder` that controls exactly what
is available. The default configuration provides:
- **No filesystem access** — no preopened directories
- **No network access** — no sockets
- **No environment variables** — empty
- **Stdout/stderr** — redirected to the host's logging system

Gadget authors can use the full Rust standard library (collections, formatting,
etc.) — only I/O operations are gated. Any real capabilities (storage, HTTP,
clipboard) go through our WIT host imports where we control exactly what's
allowed.

**Stdout/stderr → logging redirect:** `WasiCtxBuilder` supports custom output
streams. Gadget `println!` output is captured and piped into the host's
`tracing` logger tagged with the gadget ID (stdout → `info`, stderr → `warn`).
This gives gadget authors a "just works" debugging path alongside the proper
`logging::log()` WIT import.

### 5.5 Type Conversion at the WASM Boundary

`wasmtime::component::bindgen!` generates its own Rust types from the WIT
definitions. These are structurally similar to but distinct from the native
types in `search::types`. Conversion happens at the adapter boundary via
`From`/`Into` implementations in the `wasm` module.

Key mappings:
- `wasm::CatalogEntry` → `search::types::CatalogEntry`
- `wasm::PostAction` → `search::types::PostAction`
- `wasm::ActionId` → `search::types::ActionId`
- `wasm::EntryIcon` → `search::types::EntryIcon` (`asset-icon` paths are
  relative to the gadget archive root and served via `torchsnap-gadget://`)
- `wasm::Action` → `search::types::Action` (no `keybinding` field — keybindings
  for well-known `ActionId`s are filled by the host)

The WIT `action-id` variant mirrors the native `ActionId` enum
(open, copy, reveal, open-with, delete, open-settings, custom). Type-safe
across the boundary.

Custom-UI display is reachable from WASM via the `search-response::custom-ui`
and `inline-ui` variants of the WIT `search-response` (see §5.7) — the
host mounts a frontend view component referenced by name. The
`PostAction::ShowCustomUI` host-only path still exists for native
gadgets; WASM gadgets use the search-response route instead.

### 5.6 Open Questions — WIT Design

1. **Settings reactivity (resolved):** The host calls the guest export
   `lifecycle::on-setting-changed(key, value)` whenever any of the gadget's
   settings change (ADR 0029). Coalescing happens host-side via
   `CoalescingDispatcher` (ADR 0026) so rapid writes deduplicate to the
   most recent value.

2. **Cancellation in `search()` (resolved):** Stale `search()` calls are
   elided host-side — newer queries supersede older ones at the bridge
   boundary. Gadgets do not see a cancellation token. `Mutex<Store>`
   serialization (§9) plus call-return semantics (§5.7) make per-keystroke
   cancellation a host-side concern, not a guest concern.

3. **Async host calls (resolved):** Blocking host imports run on a
   dedicated tokio blocking pool — `search`/`execute` are dispatched there
   so the guest can issue synchronous HTTP/SQL/command calls without
   stalling the launcher's async runtime.

4. **Binary data in results (partially resolved):** `entry-icon::data-url`
   passes base64 strings; `entry-icon::asset-icon` passes paths relative
   to the gadget archive root, served via the `torchsnap-gadget://`
   protocol. Large in-archive blobs are reachable through the `assets`
   interface (ADR 0039). Mutable per-gadget blob storage (large clipboard
   images, etc.) — a sibling directory to `gadget-home/<id>/sql/` — is
   still TODO.

### 5.7 Simplified Search Model — Call-Return, No Streaming

**Finding:** Analysis of all existing gadgets reveals that **every gadget sends
exactly one batch of results per `search()` call.** None use the `ResultChannel`
for progressive streaming. The native gadget system's `mpsc` channel + async
receiver pattern is architecturally over-engineered for the current reality.

**Consequence for WASM gadgets:** The search interface is a pure call-return
function — `search(query, prefix) -> search-response`. No `send-results` host
import, no channel IDs, no streaming protocol. The gadget builds its results
and returns them directly. This is dramatically simpler for the WASM boundary.

```wit
// search-response is a return value, not a callback
variant search-response {
    nothing,
    results(list<scored-entry>),
    custom-ui(custom-ui-response),
    inline-ui(inline-ui-response),
}
```

**Consequence for the native gadget system:** The native `Gadget` trait should
be simplified to match. The `ResultChannel` + `CancellationToken` parameters
in `search()` can be replaced with a direct return value, aligning the native
and WASM gadget interfaces. This simplification should happen as part of the
migration — when converting each gadget, update both the WASM guest and the
native trait to use the same call-return model. The `GadgetHost` search
pipeline simplifies accordingly (no `mpsc` channel, no async drain loop).

If progressive streaming is ever needed in the future (e.g., a gadget that
fetches results from a slow network API), it can be added as an *additional*
opt-in interface. But the default path should be the simpler call-return model.

### 5.8 Hello-World WIT (Minimal Subset, historical)

> **Historical.** This was the very first WIT definition shipped to
> validate the end-to-end pipeline. The current contract is much
> larger — see `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` for the
> authoritative version. Preserved here as design history.

```wit
package torchsnap:gadget@0.1.0;

// ─── Host imports (what the host provides to gadgets) ───

interface logging {
    enum log-level { trace, debug, info, warn, error }
    log: func(level: log-level, message: string);
}

// ─── Shared types ───

interface types {
    record catalog-entry {
        id: string,
        title: string,
        subtitle: option<string>,
        icon: option<entry-icon>,
        keywords: list<string>,
        actions: list<action>,
    }

    variant entry-icon {
        hero-icon(string),
        data-url(string),
        asset-icon(string),
        emoji(string),
    }

    record action {
        id: string,
        label: string,
    }

    enum post-action {
        nothing,
        dismiss,
        keep-open,
    }
}

// ─── The gadget world ───

world gadget {
    import logging;
    use types.{catalog-entry, post-action};

    export enable: func();
    export disable: func();
    export entries: func() -> list<catalog-entry>;
    export execute: func(entry-id: string, action-id: string) -> result<post-action, string>;
}
```

This is deliberately minimal. Missing from the hello-world WIT (added later):
- `search()`, `search-prefixes()` — query gadget path
- `handle-message()`, `handle-shortcut()` — custom messaging
- `on-setting-changed()` — settings reactivity
- All other host imports (storage, clipboard, HTTP, etc.)
- `ScoredEntry`, `ResultChannel` equivalents — query gadgets need these
- `ActionId` as a proper variant (simplified to string for now)

### 5.6 Binding Mechanics

Both sides consume the same `.wit` files by path, but the tooling differs:

**Host side (wasmtime):**

```rust
// In src-tauri, generates typed Rust bindings at compile time.
// Produces: a `Gadget` type with methods like `call_enable()`,
// `call_entries()`, etc., and a trait to implement for host imports.
wasmtime::component::bindgen!({
    path: "../gadgets/gadget-sdk/wit",
    world: "gadget",
    async: false,  // start synchronous, revisit for async host calls later
});

// The generated code gives us:
// - `Gadget::instantiate()` — loads a component into a store
// - `gadget.call_enable(&mut store)` — call guest exports
// - `gadget.call_entries(&mut store)` — returns Vec<CatalogEntry>
// - A `logging::Host` trait we implement on our store state
```

The host implements the `logging::Host` trait on its store state struct:

```rust
struct GadgetState {
    gadget_id: String,
    // ... other per-gadget state
}

impl logging::Host for GadgetState {
    fn log(&mut self, level: LogLevel, message: String) {
        match level {
            LogLevel::Error => tracing::error!(gadget = %self.gadget_id, "{message}"),
            LogLevel::Warn  => tracing::warn!(gadget = %self.gadget_id, "{message}"),
            LogLevel::Info  => tracing::info!(gadget = %self.gadget_id, "{message}"),
            LogLevel::Debug => tracing::debug!(gadget = %self.gadget_id, "{message}"),
            LogLevel::Trace => tracing::trace!(gadget = %self.gadget_id, "{message}"),
        }
    }
}
```

**Guest side (`torchsnap-gadget-sdk`):**

```rust
// In gadgets/hello-world/src/lib.rs
// The SDK crate owns the wit-bindgen invocation; gadget code
// consumes the generated bindings through the prelude and
// registers itself via `define_gadget!`.

use torchsnap_gadget_sdk::prelude::*;
use torchsnap_gadget_sdk::{define_gadget, impl_noop_messaging, impl_noop_tasks};

struct HelloWorld;
define_gadget!(HelloWorld);

impl LifecycleGuest for HelloWorld {
    fn enable() {
        logging::log(logging::LogLevel::Info, "Hello from WASM!", &[], None);
    }

    fn disable() {
        logging::log(logging::LogLevel::Info, "Goodbye from WASM!", &[], None);
    }

    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for HelloWorld {
    fn entries() -> Vec<CatalogEntry> {
        vec![CatalogEntry {
            id: "greet".into(),
            title: "Say Hello".into(),
            subtitle: Some("Hello World WASM gadget".into()),
            icon: Some(EntryIcon::HeroIcon("hand-raised".into())),
            keywords: vec!["hello".into(), "greet".into(), "test".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Run".into(),
            }],
        }]
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        logging::log(
            logging::LogLevel::Info,
            &format!("Executed: {entry_id}"),
            &[],
            None,
        );
        Ok(PostAction::Dismiss)
    }
}

// The WIT world mandates `messaging` and `tasks` exports; the
// SDK no-op macros satisfy them for gadgets that declare
// neither RPC methods nor scheduled tasks.
impl_noop_messaging!(HelloWorld);
impl_noop_tasks!(HelloWorld);
```

Built with `cargo build --release` from the `gadgets/` virtual
workspace (`wasm32-wasip2` is the workspace-default target), producing a
`.wasm` component.

**Adding new host imports follows this pattern:**

1. Add a new `interface` to the `.wit` file in `gadgets/gadget-sdk/wit/`
2. Add `import <interface-name>;` to the `world gadget` block
3. Host side: `bindgen!` regenerates automatically — implement the new trait
4. Guest side: rebuilding the SDK crate regenerates the bindings —
   gadgets pick up the new import via `torchsnap_gadget_sdk::prelude::*`
   (or a dedicated re-export if the SDK surfaces it explicitly)

The WIT file is the single source of truth. Both sides get compile-time errors
if the contract is violated.

## 6. Gadget Loading Architecture

### 6.1 Gadget Discovery

> **Superseded by ADR 0035.** The final discovery scheme uses three
> precedence-ordered roots. The single-directory sketch below is
> preserved as design history.

```
<app_data_dir>/gadgets/
├── calculator.torchsnap
└── hello-world.torchsnap
```

At startup, the host scans each of the three search roots (resource-
bundled System, repo-relative Dev in debug builds, user-installed
User), reads each archive's `manifest.toml`, validates the path
guard, and registers the gadget with the `GadgetHost` tagged with its
`GadgetSourceKind`. The data dir for host-managed per-gadget state
lives at `<app_data_dir>/gadget-home/<gadget-id>/` — separate from
`<app_data_dir>/gadgets/` which carries only gadget code.

### 6.2 WASM Gadget Adapter

A `WasmGadget` struct wraps a wasmtime component instance. Whether it
implements the existing `Gadget` trait directly or the `GadgetHost` is
refactored to accommodate WASM gadgets more naturally is an open design
question — **GadgetHost modifications are explicitly on the table** if they
simplify the WASM integration or improve the overall architecture.

```
GadgetHost
├── NativeGadget (app-launcher)       ← compiled in
├── NativeGadget (system-commands)    ← compiled in
├── WasmGadget (emoji-picker)         ← wraps wasmtime instance
├── WasmGadget (calculator)           ← wraps wasmtime instance
└── ...
```

The `WasmGadget` adapter translates each gadget method call into the
corresponding WASM guest export call, and implements the WIT host imports by
delegating to the real Tauri APIs.

### 6.3 Frontend Asset Loading

Resolved differently from the original sketch: archives are **not**
extracted to disk. The host registers a custom `torchsnap-gadget://`
URI scheme (`src-tauri/src/wasm/protocol.rs`) that streams files
directly out of the archive (or development directory) on demand,
with CORS and content-type set per request. Frontend dynamic
`import()` factories in `src/gadgets/wasmGadgetLoader.ts` resolve
to URLs of the form
`torchsnap-gadget://localhost/<gadget-id>/<path>`.

This eliminates the cache-invalidation question entirely: the
archive on disk is the only source of truth.

## 7. Frontend Gadget SDK

### 7.1 Gadget Author Build Pipeline

Gadget authors need a way to write React/TypeScript components that:
- Import from `@torchsnap/keybindings`, `@torchsnap/components`, `@torchsnap/types`
- Get bundled into a single ES module per view component
- Externalize the SDK imports (they're provided by the host app at runtime)

This calls for a **gadget template project** (Rust-based) with:
- A plain Rust crate for the WASM backend, built with
  `cargo build --release --target wasm32-wasip2` (the `gadgets/` virtual
  workspace defaults the target via `gadgets/.cargo/config.toml`).
  `cargo-component` is **not** used — the SDK crate owns
  `wit_bindgen::generate!`, gadget crates consume the bindings through
  `torchsnap_gadget_sdk::prelude::*` and `define_gadget!`.
- A `tsconfig.json` pointing at SDK type declarations
- A Vite/rolldown config that bundles each view into a standalone ES module with
  externalized SDK imports
- A `just package-gadget` recipe that produces the final `.torchsnap` zip

The official template targets **Rust** as the primary gadget language. Gadget
authors using other WASM-targeting languages (Go, C, etc.) can write their own
build tooling — the `.torchsnap` archive format and WIT contract are the
stable interface, not the build system.

### 7.2 SDK Package

The SDK types and shared components would be extracted into a publishable
package (or a local workspace package). The host app and gadget template both
depend on it. This ensures type compatibility.

```
@torchsnap/gadget-sdk
├── types.ts         # SourcedEntry, ActionId, FooterState, etc.
├── components/      # Shared UI components gadgets can use
├── keybindings/     # Keybinding engine
└── hooks/           # useGadgetStream, etc.
```

**Open question (deferred):** How are SDK externals resolved at runtime?
Options:
- Importmap in the HTML (e.g., `"@torchsnap/types": "/sdk/types.js"`)
- Global variable injection (less clean)
- Shared chunk that Vite already produces (current approach for built-in gadgets)

Decision deferred until we reach the frontend integration phase — needs hands-on
experimentation to find the cleanest approach.

## 8. Migration Strategy

### 8.1 Parallel Execution

The `GadgetHost` already works with `Box<dyn Gadget>`. Native and WASM gadgets
coexist naturally — the host doesn't care about the backing implementation.

### 8.2 Conversion Order

Start with a hello-world proof-of-concept, then convert real gadgets simplest-first:

| Phase | Gadget | Complexity | Why this order |
|---|---|---|---|
| 0 | **hello-world** (new) | Minimal | Pure validation of the end-to-end pipeline: WIT definition, wasmtime loading, `.torchsnap` archive parsing, manifest reading, `WasmGadget` adapter, and basic catalog search. No host capabilities needed beyond logging. Proves the infrastructure works before touching any real gadget. |
| 1 | `commands` (built-in) | Trivial | Almost no host deps — but uses `app.exit()` and `show_settings_window()`, which are host-internal. **Skip — keep native.** |
| 1 | `calculator` | Low | Self-contained math eval, SQL history. Good first candidate: exercises SQL storage, clipboard write, settings, and CustomUI + InlineUI. |
| 2 | `emoji` | Low-medium | Embedded data (compile-time `include_str!`), nucleo matching, frecency. Exercises data embedding in WASM, frecency host API. |
| 3 | `open-url` | Medium | URL detection logic, metadata service, always-on query. Exercises HTTP/metadata host imports. |
| 3 | `bangs` | Medium | Network fetch, SQL storage, URL encoding. Similar capability profile to open-url. |
| 4 | `clipboard` | High | Most complex: clipboard watching, dual storage (SQL + blob), streaming `handle_message`, platform-specific paste, large data handling. |
| 5 | `app-launcher` | High | Platform-specific app discovery, icon extraction, background refresh. Heavy platform abstraction dependency. |
| 5 | `system-preferences` | High | Platform-specific settings pane discovery, SF Symbol icons. |
| 6 | `system-commands` | Special | Entirely platform-specific (osascript, pmset). **May stay native** — the value of WASMifying shell command execution is debatable. |

### 8.3 Step-by-Step Process for Each Gadget

1. **Define/extend WIT interfaces** needed by this gadget
2. **Implement host-side imports** in the `WasmGadget` adapter
3. **Create the WASM guest crate** under `gadgets/<id>/`, built with plain
   `cargo build --release` (no `cargo-component`)
4. **Port the gadget logic** from the native implementation
5. **Bundle frontend assets** into the `.torchsnap` archive
6. **Integration test** — both native and WASM versions active, verify identical behavior
7. **Remove the native implementation** once the WASM version is validated
8. **Clean up** — remove any host code that was only needed by the native version

### 8.4 Project Layout

The host (`src-tauri/`) is an independent cargo crate. Gadget crates live
under `gadgets/` inside a virtual cargo workspace (`gadgets/Cargo.toml`),
with `wasm32-wasip2` as the workspace-default target
(`gadgets/.cargo/config.toml`). Host and gadgets still never build in one
invocation; build orchestration is handled by just recipes.

WIT definitions live inside the `torchsnap-gadget-sdk` crate
(`gadgets/gadget-sdk/wit/`) so gadget crates pick them up through the
SDK and the host references them by relative path from `src-tauri/`.

```
torchsnap/
├── src-tauri/                      # standalone Rust crate (Tauri host app)
│   ├── Cargo.toml
│   └── Cargo.lock
├── gadgets/                        # virtual cargo workspace
│   ├── Cargo.toml                  # workspace manifest + release profile
│   ├── Cargo.lock                  # single lockfile for all gadgets
│   ├── .cargo/config.toml          # default target = wasm32-wasip2
│   ├── gadget-sdk/                 # torchsnap-gadget-sdk crate
│   │   ├── Cargo.toml
│   │   ├── wit/
│   │   │   └── torchsnap-gadget.wit
│   │   └── src/lib.rs
│   └── hello-world/                # example gadget crate
│       ├── Cargo.toml
│       ├── manifest.toml           # gadget manifest (used by DirectorySource)
│       └── src/lib.rs
├── packages/
│   └── gadget-sdk/                 # TypeScript SDK (`@torchsnap/gadget-sdk`)
├── src/                            # frontend (unchanged)
├── package.json
└── ...
```

The WIT file is referenced by relative path from both sides:
- Host: `wasmtime::component::bindgen!({ path: "../gadgets/gadget-sdk/wit" })`
- SDK crate: `wit_bindgen::generate!({ path: "wit", ... })` (relative to
  the SDK crate root); individual gadget crates don't invoke
  `generate!` themselves — they consume the bindings through
  `torchsnap_gadget_sdk::prelude::*` + `define_gadget!`.

Gadget crates during initial development live in `gadgets/`. The template for
external gadgets will be a standalone repo later.

### 8.5 Infrastructure Milestones

These milestones build on each other. The hello-world gadget (Phase 0) is the
forcing function that proves each piece works end-to-end.

1. **Minimal WIT definition** — define the bare-minimum `torchsnap:gadget` world
   - Lifecycle exports: `enable`, `disable`
   - Search exports: `entries`, `execute`
   - Host imports: `logging` only (simplest possible host function, proves
     bidirectional communication: host→guest calls AND guest→host calls)

2. **Hello-world guest crate** — a plain `cargo build` Rust project in `gadgets/`
   - Implements the WIT world
   - Calls `log(info, "Hello from WASM!")` in `enable()`
   - Returns one hardcoded catalog entry from `entries()`
   - Logs and returns `Dismiss` from `execute()`
   - Validates that `cargo build --release --target wasm32-wasip2` produces a valid `.wasm` component

3. **WASM host runtime** — `wasmtime` integration in the Tauri app
   - Add `wasmtime` dependency with `component-model` feature
   - `wasmtime::component::bindgen!` against the WIT definitions
   - `WasmGadgetInstance` struct wrapping `Store` + typed component instance
   - `Mutex<Store>` for initial threading (revisited later)
   - One shared `wasmtime::Engine` for all WASM gadgets (stored in `GadgetHost`
     or Tauri state)

4. **WasmGadget adapter** — bridges WASM instance to the gadget system
   - Implements enough of the gadget interface for catalog search + execute
   - Implements the `logging` host import (maps to `tracing::info!`)
   - Reads WASM bytes via `GadgetSource::read_wasm()`

5. **Integration** — hello-world shows up in search results
   - Load hello-world via `DirectorySource` (development mode)
   - Register `WasmGadget` with `GadgetHost`
   - Verify it appears in search, execute works, log output visible

6. **Archive loader** — `.torchsnap` zip reading (`ArchiveSource`)
   - Gadget directory scanning at `$APPDATA/torchsnap/gadgets/`
   - WASM binary extraction from manifest-declared path
   - Frontend asset extraction to cache directory (deferred until needed)

7. **Gadget template** — formalize the hello-world into a reusable template
   - Plain `cargo build` project structure (no `cargo-component`)
   - WIT bindings via the SDK crate, build scripts
   - Archive packaging script (produces `.torchsnap`)
   - Future: add frontend Vite config when frontend gadgets are tackled

8. **Frontend dynamic loading** (deferred until a real gadget with UI is converted)
   - Asset protocol serving of extracted gadget JS
   - Dynamic `import()` based on manifest paths
   - SDK externalization

## 9. WASM Store Threading Model

WASM component instances are **stateful** — linear memory persists across calls,
so a gadget can build up state in `enable()` and use it in `entries()`/`execute()`
naturally. However, wasmtime's `Store` is `!Send + !Sync`, meaning a single
store cannot be called from multiple threads concurrently.

The current native `Gadget` trait requires `Send + Sync`, and the host calls
into gadgets from arbitrary tokio/blocking threads. The `WasmGadgetBridge`
adapter wraps the store in a `Mutex` to satisfy this constraint.

**Decision: `Mutex<Store>`** — this is the correct and permanent model, not a
compromise. WASM execution is inherently single-threaded per instance: only one
guest function can execute at a time regardless of how host-side access is
serialized. A dedicated-thread-per-gadget model (channel + message queue) was
considered but adds complexity for no concurrency benefit — calls are serialized
either way. The Mutex is simpler, equally correct, and has no performance
disadvantage.

Cross-gadget parallelism is not affected: each gadget has its own `Store` with
its own `Mutex`, so gadget A's `search()` cannot block gadget B.

## 10. Risks and Considerations


### 9.1 Binary Size

wasmtime is a significant dependency. Expect 5-15 MB increase in the final
binary. The project already includes rusqlite (bundled SQLite) and reqwest, so
it's not unprecedented, but worth measuring early.

### 9.2 Startup Time

WASM compilation on first load can be slow. Wasmtime supports ahead-of-time
compilation to native code (`Engine::precompile_component`), producing a
`.cwasm` file. The host can cache this alongside the `.torchsnap` archive and
only recompile on archive change.

### 9.3 Performance

WASM function calls have overhead compared to native Rust. For search (called
on every keystroke), this matters. Mitigations:
- Catalog gadgets: `entries()` is called frequently but returns cached data —
  the host can cache the result and only re-call on a signal
- Query gadgets: `search()` already runs on `spawn_blocking` — WASM overhead
  is amortized within the blocking task
- Measure before optimizing

### 9.4 Platform-Specific Gadgets

`system-commands`, `app-launcher`, and `system-preferences` are deeply
platform-specific. Two options:
- Keep them native permanently (pragmatic)
- Expose platform capabilities as host imports and let them run in WASM
  (architecturally pure but high effort for low benefit)

Recommendation: keep these native. The primary value of WASM gadgets is for
**third-party extensibility** — first-party platform integrations don't benefit
from sandboxing or cross-platform distribution.

### 9.5 Debugging

WASM debugging is harder than native Rust. Gadget authors will need:
- Good error messages from host functions (not just "trap")
- `torchsnap:host/logging` for printf-style debugging
- Potential DWARF debug info in development builds

## 11. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| (prior) | WIT + Component Model via wasmtime | Type safety, versioned interfaces, multi-language support |
| 2026-04-03 | Archive extension: `.torchsnap` | Brand-aligned, unambiguous. `.torch` considered but `.torchsnap` is clearer. |
| 2026-04-03 | WASM filename declared in manifest, not hardcoded | Flexibility for gadget authors; avoids naming conflicts in multi-crate setups |
| 2026-04-03 | Settings reactivity via host→guest callback | `on-setting-changed(key, value)` guest export. Simple, no polling, fits WASM call-in model. |
| 2026-04-03 | Start with hello-world gadget before converting real gadgets | Validates entire pipeline (WIT, wasmtime, archive, adapter) with zero complexity. De-risks infrastructure before real migration. |
| 2026-04-03 | Official gadget template targets Rust | Most mature WIT/Component Model tooling. Other languages can target the `.torchsnap` format independently. |
| 2026-04-03 | GadgetHost modifications are on the table | If refactoring GadgetHost makes WASM integration cleaner, we do it rather than force-fitting into the existing API. |
| 2026-04-03 | Enable/disable is a host-level concern | Every gadget gets host-managed enable/disable. Gadget provides `on-enable`/`on-disable` callbacks. No `enabled` in gadget settings. |
| 2026-04-03 | Shortcuts are host-managed storage | Manifest declares defaults; host stores actual bindings. Gadget only receives `handle-shortcut(id)`. |
| 2026-04-03 | Settings section is flat key-value defaults | No `defaults` sub-key. The `[settings]` section IS the defaults map. |
| 2026-04-03 | Two frontend bundles per gadget (launcher + settings) | Separate webviews need separate bundles to avoid cross-window side effects and unnecessary code loading. |
| 2026-04-03 | Disabled gadgets skip WASM instantiation | Only manifest is parsed. Full instantiation on enable, full teardown + drop on disable. Saves memory and startup time. |
| 2026-04-03 | Gadget metadata in manifest for settings UI | `name`, `description`, `icon` in `[gadget]` — host renders settings sidebar without gadget frontend code. |
| 2026-04-03 | No search mode field | Gadgets implement full export surface; empty returns for unused modes. Matches native trait design. |
| 2026-04-03 | No cargo workspace — independent projects | Host and gadgets have separate targets; build orchestration via just recipes. WIT files shared by path in `wit/` at repo root. (Superseded: gadgets now share a virtual cargo workspace at `gadgets/Cargo.toml`, and the WIT file lives under `gadgets/gadget-sdk/wit/`.) |
| 2026-04-03 | `logging` as first host import | Simplest host function, proves bidirectional communication in hello-world |
| 2026-04-03 | `Mutex<Store>` as permanent threading model | WASM is inherently single-threaded per instance; dedicated-thread model adds complexity for no concurrency benefit. |
| 2026-04-03 | One shared `wasmtime::Engine` | All WASM gadgets share an engine instance — saves compilation cache and memory |
| 2026-04-03 | Drop `cargo-component`, use `wit-bindgen` + `cargo build --target wasm32-wasip2` | cargo-component 0.21.1 pins wit-bindgen to 0.41 internally, causing version skew. Direct `wit-bindgen 0.54` + standard cargo is simpler, latest features, no extra subcommand. |
| 2026-04-03 | Call-return search model, no streaming callbacks | All current gadgets send exactly one result batch per search(). Drop ResultChannel/mpsc in favor of direct return values. Simplify native Gadget trait to match. |
| 2026-04-05 | Which gadgets stay native | `commands`, `system_commands`, `app_launcher`, `system_preferences` — deep platform APIs or host operations |
| 2026-04-05 | Host capabilities added on-demand | Build WIT imports as each gadget is migrated, not all upfront |
| 2026-04-05 | Calculator is first migration target | Exercises SQL + clipboard + settings — representative slice of capabilities |
| TBD | Frontend SDK externalization strategy | Deferred until frontend gadget integration phase |

## 12. Implementation Status

> Last updated: 2026-04-30

### Done

| Component | Location | Notes |
|---|---|---|
| WIT contract | `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` | Full surface — see §3.2 table for the per-interface map |
| WASM runtime | `src-tauri/src/wasm/runtime/` | `WasmRuntime` (shared `Engine` + compiled-`Component` cache) + per-instance `Mutex<Store>` |
| WasmGadgetBridge | `src-tauri/src/wasm/bridge.rs` | Compile-at-load / instantiate-on-enable lifecycle (ADR 0033); implements the native `Gadget` trait |
| Gadget sources | `src-tauri/src/wasm/source.rs` | `DirectorySource` (dev) + `ArchiveSource` (production) |
| Gadget discovery | `src-tauri/src/wasm/discovery.rs` | Three roots: bundled System, repo-relative Dev (debug), user-installed User (ADR 0035) |
| Path safety | `src-tauri/src/wasm/path_safety.rs` | `validate-gadget-path` guard for assets/protocol/archive reads |
| Asset protocol | `src-tauri/src/wasm/protocol.rs` | `torchsnap-gadget://localhost/<id>/<path>` streams from archive, CORS + content-type |
| Manifest parsing | `src-tauri/src/wasm/manifest.rs` | Full `manifest.toml` validation incl. permission tables |
| Structured logging + spans | `src-tauri/src/wasm/logging/` | Channel, spans, ring-buffer storage, DevTools commands |
| Type conversions | `src-tauri/src/wasm/bindings.rs` | WIT ↔ native type `From` impls |
| SQL storage | WIT `sql` interface | ADR 0031. DB at `<app_data_dir>/gadget-home/<id>/sql/storage.sqlite3`; migrations applied before `enable()` |
| Settings read + reactivity | WIT `settings` + `lifecycle::on-setting-changed` | ADR 0029 |
| Custom RPC | WIT `messaging::handle-message` | ADR 0030 (request/response only; no streaming) |
| Scheduled tasks | WIT `tasks::run-task` + manifest cron | ADR 0032 |
| Opener (URL/path/reveal) | WIT `opener` interface | ADR 0037 |
| HTTP fetch | WIT `http` interface | ADR 0038 |
| Per-gadget asset reads | WIT `assets` interface | ADR 0039 |
| Subprocess execution | WIT `command` interface | ADR 0040 |
| Filesystem reads (manifest-gated) | WIT `fs` interface | Glob allowlist + canonicalization |
| Frecency (read-only) | WIT `frecency` interface | Host records writes automatically; `top-items`/`is-enabled` for UI |
| Website metadata service | WIT `website-metadata` interface | Shared cross-gadget cache; `cached`/`blocking` lookup modes |
| Platform/arch + path resolution | WIT `platform` + `paths` | Informational, no permission required |
| Distribution | `gadgets/bundled.toml` + `stage-bundled-gadgets` recipe | ADR 0035; `target/bundled-gadgets/` consumed by Tauri `resources` |
| Trust model | — | ADR 0036 (deferred signing) |
| Hello-world gadget | `gadgets/hello-world/` | Catalog + query search + custom frontend view + spans |
| Calculator gadget | `gadgets/calculator/` | Migrated — exercises SQL + clipboard + settings + custom UI |
| Open-url gadget | `gadgets/open-url/` | Migrated — exercises HTTP + opener + website-metadata |
| Bangs gadget | `gadgets/bangs/` | Migrated — exercises HTTP + SQL + opener + assets |
| Emoji-picker gadget | `gadgets/emoji-picker/` | Migrated — exercises frecency-read + assets |
| Zerotier gadget | `gadgets/zerotier/` | Migrated — exercises command + http + fs + paths (ADR 0041); see todos for known issues |
| Build tooling | `just/gadgets.just` | `build-gadget`, `check-gadget`, `package-gadget`, `check-wit`, `fmt-wit`, `stage-bundled-gadgets` |
| Gadget trait refactor | `src-tauri/src/gadgets/mod.rs` | `enable()`/`disable()`/`setting_changed()` lifecycle |
| Host-managed lifecycle | `src-tauri/src/gadget_host.rs` | `GadgetSlot` with `AtomicBool` + `CoalescingDispatcher` (ADR 0026) |
| Frontend dynamic loading | `src/gadgets/wasmGadgetLoader.ts` | `initGadgetSdk()`, manifest-driven registration, CSS scoping |
| Gadget template | `gadgets/template/` | Reusable starting point with Vite + TSX + Tailwind frontend |

### Not Yet Implemented

- **Mutable blob storage** — large per-gadget binary blobs (e.g. clipboard
  images). Planned as a sibling directory to `gadget-home/<id>/sql/`.
- **`handle-shortcut` for WASM** — manifest-declared global shortcuts are
  not yet routed to WASM guests.
- **Clipboard gadget migration** — biggest remaining migration; blocked
  primarily on blob storage.
- **`api-version` field** — deferred until 1.0.0 stability.
- **Gadget signing / trust UX** — deferred per ADR 0036.

### What Stays Native

- **`commands`** — calls `app.exit()`, window management (host operations)
- **`system_commands`** — subprocess execution that pre-dates the `command`
  WIT interface; may be migratable now via ADR 0040
- **`app_launcher`** — AppKit app discovery + icon extraction (deep macOS API)
- **`system_preferences`** — plist parsing, macOS System Settings integration

### Next Milestone

Clipboard gadget migration — requires the still-missing blob storage host
import; calculator/open-url/bangs/emoji-picker have already validated the
SQL/HTTP/opener/frecency surfaces.
