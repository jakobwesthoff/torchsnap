# Self-Contained Plugin System — Strategy Document

> Living document. Updated as design decisions are made during discussion.

**Testing policy:** All new code in the WASM plugin system must have extensive
test coverage. Every struct, every validation path, every error case. This is
foundational infrastructure — correctness matters more than velocity.

## 1. Vision

Plugins become self-contained modules — `.torchsnap` zip archives containing a
WASM backend component and pre-bundled frontend assets. They are discovered,
loaded, and activated at runtime without recompilation of the host. The existing
native plugin system runs in parallel during migration and is removed once all
plugins have been converted.

## 2. Prior Research

Decision already made in `docs/research/wasm-wit-plugin-system.md`:

- **WIT + Component Model** via `wasmtime` (not Extism, not raw wasip1)
- Compile-time type safety across host/guest boundary via `wasmtime::component::bindgen!`
- Plugin authors write Rust (most mature), Go, or potentially JS/Python
- The existing `Plugin` trait comment already notes it was "structured so it can
  later become the boundary for a WASM plugin interface"

## 3. Current System Inventory

### 3.1 Plugin Trait (unified)

Single `Plugin` trait in `src-tauri/src/plugins/mod.rs` covering catalog,
query, and hybrid modes. Key methods:

| Method | WASM-relevant notes |
|---|---|
| `id()` | Trivial string export |
| `entries()` | Returns `Vec<CatalogEntry>` — serializable, crosses boundary cleanly |
| `search(query, prefix, results, cancel)` | `ResultChannel` and `CancellationToken` need host-side proxying |
| `execute(entry_id, action_id, app)` | `AppHandle` does NOT cross WASM — must be replaced with capability calls |
| `handle_message(method, payload, channel)` | JSON in/out + streaming channel — needs host-side channel proxy |
| `setup(app, ctx)` | `PluginContext` (settings, notifier, frecency) must become host imports |
| `initialize_settings(settings)` | Declarative — could become a manifest field instead of code |
| `shortcuts()` | Returns static data — manifest or simple export |
| `handle_shortcut(id, app)` | Same `AppHandle` problem as `execute` |
| `teardown()` | Simple signal |

### 3.2 Host Capabilities Used by Plugins

Analysis of what each plugin actually needs from the host:

| Capability | Used by | WIT interface candidate |
|---|---|---|
| SQL storage (open, execute, query) | calculator, clipboard, bangs | `torchsnap:storage/sql` |
| File blob storage (store, load, delete) | clipboard | `torchsnap:storage/blob` |
| Clipboard read/write | calculator, emoji, bangs, open-url, clipboard | `torchsnap:platform/clipboard` |
| Open URL in browser | bangs, open-url | `torchsnap:platform/opener` |
| HTTP GET/POST | bangs (DDG bang fetch) | `torchsnap:network/http` |
| Website metadata (title, favicon) | bangs, open-url | `torchsnap:network/metadata` |
| Settings read | all with settings | `torchsnap:settings/read` |
| Settings watch/subscribe | calculator, clipboard, bangs | `torchsnap:settings/watch` |
| Frecency scoring | emoji | `torchsnap:frecency` |
| App discovery + launch | app-launcher | `torchsnap:platform/apps` |
| System preferences discovery | system-preferences | `torchsnap:platform/system-prefs` |
| Subprocess execution | system-commands | `torchsnap:platform/subprocess` |
| App exit / show settings window | commands (built-in) | Likely stays native-only |
| Icon cache | app-launcher, system-preferences | `torchsnap:platform/icons` |
| Logging | all | `torchsnap:logging` |

### 3.3 Frontend Plugin Surface

Each plugin can provide:

- **Views** (`Record<string, ComponentType<PluginViewProps>>`) — custom UI replacing result list
- **Inline views** (`Record<string, ComponentType<InlineViewProps>>`) — widget above result list
- **Settings component** (`ComponentType<PluginSettingsProps>`) — settings panel

Components must be ES modules with `default` exports. They consume a stable
props interface and communicate with the backend via `sendMessage` (already
JSON-based, natural fit for WASM).

The registry (`src/plugins/registry.ts`) is explicitly marked as a TODO for
dynamic resolution. The `pluginComponent.tsx` lazy-loading infrastructure
already supports dynamic `import()` factories.

### 3.4 Build System

- Vite 8 (rolldown-based) with manual chunk splitting that already isolates
  plugin code from shared code
- `@torchsnap/keybindings`, `@torchsnap/components`, `@torchsnap/types` path
  aliases form the implicit "plugin frontend SDK"
- Tauri's `assetProtocol` scope already includes `$APPDATA/**` — usable for
  loading plugin assets from disk

## 4. Plugin Archive Format

### 4.1 `.torchsnap` Zip Archive

File extension: **`.torchsnap`** (alternative considered: `.torch`).

The WASM binary filename is declared in the manifest — not hardcoded. This
allows plugin authors to use meaningful names and avoids conflicts in multi-crate
workspaces.

```
emoji-picker.torchsnap (zip)
├── manifest.toml           # Plugin metadata + declarations
├── emoji_picker.wasm       # WASM component (path declared in manifest)
├── frontend/
│   ├── launcher.js         # ES module: views + inline views (loaded in launcher webview)
│   └── settings.js         # ES module: settings component (loaded in settings webview)
└── assets/                 # Optional static assets (icons, data files)
    └── icon.svg
```

### 4.2 `manifest.toml`

```toml
[plugin]
id = "clipboard-manager"                # Stable identifier (lowercase, alphanumeric + hyphens)
name = "Clipboard Manager"              # Human-readable display name
description = "Clipboard history with search and paste"  # Shown in settings section header
version = "0.1.0"                       # Plugin's own version (display, update checks)
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

**All `[plugin]` fields are required.** `id`, `name`, `description`, `version`,
`wasm`, and `icon` must all be present. The host uses `name`, `description`, and
`icon` to render the settings section header (`SectionHeader` component) —
without them, the plugin has no settings presence.

**Icon format** supports two modes via prefix:
- `"heroicons:<name>"` — resolved to a HeroIcon component (e.g.,
  `"heroicons:clipboard-document-list"`)
- Any other string — treated as a file path within the plugin archive/directory
  pointing to a WebP image (e.g., `"assets/icon.webp"`)

**Plugin ID is explicit, not derived from name.** The ID is the stable key for
settings namespaces (`plugins.<id>.*`), frecency storage, data directories, and
frontend registry lookups. Deriving it from `name` would break user data if the
plugin is renamed. Authors pick an ID once; the host validates format
(lowercase, alphanumeric + hyphens only) at load time.

**No search `mode` field.** All WASM plugins implement the full guest export
surface (`entries()`, `search()`, `execute()`, etc.). A catalog-only plugin
returns an empty list from `search()`; a query-only plugin returns an empty list
from `entries()`. This mirrors the native `Plugin` trait where all methods have
no-op/empty defaults. The host always calls both paths — the cost of an empty
return is negligible.

**No `api-version` yet.** Deferred until the 1.0.0 release when API stability
matters. During development the WIT contract changes freely.

**Enable/disable is a host-level concern.** There is no `enabled` key in
`[settings]`. Every plugin is enable/disable-able, managed entirely by the host.
The host:
- Stores the enabled state in its own storage (not the plugin's settings
  namespace)
- Renders the enable/disable toggle in the settings UI for every plugin
- Never calls `search`/`entries`/`execute` on disabled plugins
- **Does not instantiate the WASM module at all for disabled plugins.** Only the
  manifest is parsed (cheap). No `wasmtime::Engine`, no `Store`, no memory.

**No separate `on-enable`/`on-disable` exports.** Since the host drops and
re-instantiates the entire WASM module on enable/disable, `setup()` *is* the
enable handler and `teardown()` *is* the disable handler. There's no persistent
WASM state between disable→re-enable — the plugin starts fresh every time.

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
> `torchsnap-plugin.wit`, and the WASM instance is currently kept alive
> across toggles rather than dropped and re-instantiated. This is more
> flexible: plugins can do expensive one-time setup in `enable()` without
> paying re-instantiation cost on every toggle. The host side uses
> `PluginSlot` (with `AtomicBool` + `CoalescingDispatcher`) in
> `plugin_host.rs` to manage the enabled state and dispatch
> `setting_changed()` calls.
>
> **Open question:** We may still move to destroying WASM instances on
> disable (matching the original design) for memory savings. The
> `enable()`/`disable()` guest exports would remain regardless — they
> serve as lifecycle hooks whether the instance is long-lived or
> re-created on each toggle.

The `[plugin]` section provides `name`, `description`, and `icon` so the host
can render the settings sidebar entry without any frontend code from the plugin.
Every plugin gets a settings entry even if it has no custom settings — the
enable/disable toggle is always present.

**Shortcuts are host-managed.** The manifest declares shortcuts (ID, label,
default keybinding), but the actual keybinding value is stored and managed by
the host. The plugin receives `handle-shortcut(id)` when a shortcut fires —
it never sees or manages the keybinding itself. The settings UI renders shortcut
configuration below the plugin's custom settings section or in a global
shortcuts area (TBD).

**Settings are flat defaults.** The `[settings]` section is a direct key-value
map of default values. No nesting, no `defaults` sub-key. The host applies
these as defaults (only filling missing keys, never overwriting user values) at
plugin load time. Settings migrations (rename/remove keys) will be handled
through a separate mechanism if needed.

**Two frontend bundles (launcher + settings).** Torchsnap uses two separate
webviews — the launcher window and the settings window. Loading a single bundle
in both would pull unnecessary code into each window and risk cross-window side
effects (the exact problem the current `manualChunks` Vite config solves). So
each plugin produces up to two ES module bundles:
- `launcher-bundle` — contains views and inline views (loaded in the launcher)
- `settings-bundle` — contains the settings component (loaded in settings window)

Either can be omitted if the plugin has no components for that window. Named
exports map component names from each bundle.

### 4.4 Plugin Source Abstraction

During development, requiring a `.torchsnap` zip on every change is impractical.
The loader uses a source abstraction:

```
trait PluginSource
├── DirectorySource  — reads manifest.toml + files from a folder on disk
└── ArchiveSource    — reads from a .torchsnap zip
```

Both yield the same `Manifest` struct and provide access to the WASM binary and
frontend assets. The `PluginHost` (and `WasmPlugin` adapter) are agnostic to
which source loaded the plugin.

> **Superseded by ADR 0035.** The discovery and layout details below
> have been finalized as three precedence-ordered search roots
> (`<resource_dir>/plugins/` for bundled System plugins,
> `<CARGO_MANIFEST_DIR>/../plugins/` for Dev plugins in debug builds,
> `<app_data_dir>/plugins/` for user-installed plugins). See ADR 0035
> for the authoritative rules; this paragraph is preserved for
> historical design context.

**Development workflow:** the debug-build loader automatically scans
`<CARGO_MANIFEST_DIR>/../plugins/` — no config or CLI flag needed.
**Production:** `<resource_dir>/plugins/` for bundled `.torchsnap`
archives and `<app_data_dir>/plugins/` for user-installed ones.

### 4.3 Why Zip?

- Well-understood format with excellent Rust support (`zip` crate)
- Single-file distribution — no directory structure to manage
- Can be read in-memory without extracting to disk (though extraction to a
  cache dir is fine for frontend assets that the WebView needs to load via URL)
- The frontend JS files need to be accessible via a URL for dynamic `import()`.
  The ADR-0035 install flow keeps archives intact on disk; frontend
  assets are served via a custom `torchsnap-plugin://` protocol
  handler that streams them straight out of the zip.

## 5. WIT Interface Design

### 5.1 Layered Interface Architecture

Rather than one monolithic `plugin` world, split host capabilities into
independent WIT interfaces that can evolve separately:

```
torchsnap:plugin@0.1.0
├── world plugin               # The main world plugins target
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
- Add new host capabilities (new interfaces) without breaking existing plugins
- Version interfaces independently
- Potentially gate capabilities per-plugin (sandboxing)

### 5.2 Core Guest Exports (what plugins implement)

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
    // read-text: func() -> result<string, string>;  // future
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

// torchsnap:host/frecency
interface frecency {
    record: func(item-id: string);
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

Plugin authors can use the full Rust standard library (collections, formatting,
etc.) — only I/O operations are gated. Any real capabilities (storage, HTTP,
clipboard) go through our WIT host imports where we control exactly what's
allowed.

**Stdout/stderr → logging redirect:** `WasiCtxBuilder` supports custom output
streams. Plugin `println!` output is captured and piped into the host's
`tracing` logger tagged with the plugin ID (stdout → `info`, stderr → `warn`).
This gives plugin authors a "just works" debugging path alongside the proper
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
- `wasm::EntryIcon` → `search::types::EntryIcon`
- `wasm::Action` → `search::types::Action` (keybinding is `None` — host-managed)

The WIT `action-id` variant mirrors the native `ActionId` enum exactly
(open, copy, reveal, open-with, delete, custom). Type-safe across the boundary.

Native types that have no WIT equivalent yet (e.g., `PostAction::ShowCustomUI`,
`ActionKeybinding`) are handled when the WIT is extended for real plugins.

### 5.6 Open Questions — WIT Design

1. **Settings reactivity (decided: callback):** The host calls the guest export
   `on-setting-changed(key, value)` whenever any of the plugin's settings
   change. Already part of the `torchsnap:core/lifecycle` interface. Simple,
   no polling, fits WASM's call-in model naturally.

2. **Cancellation in `search()`:** `CancellationToken` is a Tokio type. In WASM,
   options:
   - Cooperative polling: `is-cancelled() -> bool` host import, plugin checks periodically
   - Fuel-based: wasmtime fuel metering to abort runaway plugins
   - Both (belt and suspenders)

3. **Async host calls:** HTTP requests from within `search()` are blocking in
   native plugins (they run on `spawn_blocking`). In WASM, the host function
   implementation can use async wasmtime to suspend the WASM instance while
   the actual HTTP request runs on Tokio. This is the cleanest path but requires
   wasmtime's `async` support (stable, behind a feature flag).

4. **Binary data in results:** `EntryIcon::DataUrl` passes base64 strings.
   `EntryIcon::AssetIcon` passes file paths. Both work across WASM. But large
   clipboard entries (images) in the clipboard plugin use `FileStorage` — WASM
   plugins would need blob storage host imports. Already sketched as
   `torchsnap:host/storage` but needs `blob-store` / `blob-load` additions.

### 5.7 Simplified Search Model — Call-Return, No Streaming

**Finding:** Analysis of all existing plugins reveals that **every plugin sends
exactly one batch of results per `search()` call.** None use the `ResultChannel`
for progressive streaming. The native plugin system's `mpsc` channel + async
receiver pattern is architecturally over-engineered for the current reality.

**Consequence for WASM plugins:** The search interface is a pure call-return
function — `search(query, prefix) -> search-response`. No `send-results` host
import, no channel IDs, no streaming protocol. The plugin builds its results
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

**Consequence for the native plugin system:** The native `Plugin` trait should
be simplified to match. The `ResultChannel` + `CancellationToken` parameters
in `search()` can be replaced with a direct return value, aligning the native
and WASM plugin interfaces. This simplification should happen as part of the
migration — when converting each plugin, update both the WASM guest and the
native trait to use the same call-return model. The `PluginHost` search
pipeline simplifies accordingly (no `mpsc` channel, no async drain loop).

If progressive streaming is ever needed in the future (e.g., a plugin that
fetches results from a slow network API), it can be added as an *additional*
opt-in interface. But the default path should be the simpler call-return model.

### 5.8 Hello-World WIT (Minimal Subset)

The first WIT definition covers just what the hello-world needs. We expand
it as real plugins are converted.

```wit
package torchsnap:plugin@0.1.0;

// ─── Host imports (what the host provides to plugins) ───

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

// ─── The plugin world ───

world plugin {
    import logging;
    use types.{catalog-entry, post-action};

    export enable: func();
    export disable: func();
    export entries: func() -> list<catalog-entry>;
    export execute: func(entry-id: string, action-id: string) -> result<post-action, string>;
}
```

This is deliberately minimal. Missing from the hello-world WIT (added later):
- `search()`, `search-prefixes()` — query plugin path
- `handle-message()`, `handle-shortcut()` — custom messaging
- `on-setting-changed()` — settings reactivity
- All other host imports (storage, clipboard, HTTP, etc.)
- `ScoredEntry`, `ResultChannel` equivalents — query plugins need these
- `ActionId` as a proper variant (simplified to string for now)

### 5.6 Binding Mechanics

Both sides consume the same `.wit` files by path, but the tooling differs:

**Host side (wasmtime):**

```rust
// In src-tauri, generates typed Rust bindings at compile time.
// Produces: a `Plugin` type with methods like `call_enable()`,
// `call_entries()`, etc., and a trait to implement for host imports.
wasmtime::component::bindgen!({
    path: "../plugins/plugin-sdk/wit",
    world: "plugin",
    async: false,  // start synchronous, revisit for async host calls later
});

// The generated code gives us:
// - `Plugin::instantiate()` — loads a component into a store
// - `plugin.call_enable(&mut store)` — call guest exports
// - `plugin.call_entries(&mut store)` — returns Vec<CatalogEntry>
// - A `logging::Host` trait we implement on our store state
```

The host implements the `logging::Host` trait on its store state struct:

```rust
struct PluginState {
    plugin_id: String,
    // ... other per-plugin state
}

impl logging::Host for PluginState {
    fn log(&mut self, level: LogLevel, message: String) {
        match level {
            LogLevel::Error => tracing::error!(plugin = %self.plugin_id, "{message}"),
            LogLevel::Warn  => tracing::warn!(plugin = %self.plugin_id, "{message}"),
            LogLevel::Info  => tracing::info!(plugin = %self.plugin_id, "{message}"),
            LogLevel::Debug => tracing::debug!(plugin = %self.plugin_id, "{message}"),
            LogLevel::Trace => tracing::trace!(plugin = %self.plugin_id, "{message}"),
        }
    }
}
```

**Guest side (`torchsnap-plugin-sdk`):**

```rust
// In plugins/hello-world/src/lib.rs
// The SDK crate owns the wit-bindgen invocation; plugin code
// consumes the generated bindings through the prelude and
// registers itself via `define_plugin!`.

use torchsnap_plugin_sdk::prelude::*;
use torchsnap_plugin_sdk::{define_plugin, impl_noop_messaging, impl_noop_tasks};

struct HelloWorld;
define_plugin!(HelloWorld);

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
            subtitle: Some("Hello World WASM plugin".into()),
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
// SDK no-op macros satisfy them for plugins that declare
// neither RPC methods nor scheduled tasks.
impl_noop_messaging!(HelloWorld);
impl_noop_tasks!(HelloWorld);
```

Built with `cargo build --release` from the `plugins/` virtual
workspace (`wasm32-wasip2` is the workspace-default target), producing a
`.wasm` component.

**Adding new host imports follows this pattern:**

1. Add a new `interface` to the `.wit` file in `plugins/plugin-sdk/wit/`
2. Add `import <interface-name>;` to the `world plugin` block
3. Host side: `bindgen!` regenerates automatically — implement the new trait
4. Guest side: rebuilding the SDK crate regenerates the bindings —
   plugins pick up the new import via `torchsnap_plugin_sdk::prelude::*`
   (or a dedicated re-export if the SDK surfaces it explicitly)

The WIT file is the single source of truth. Both sides get compile-time errors
if the contract is violated.

## 6. Plugin Loading Architecture

### 6.1 Plugin Discovery

> **Superseded by ADR 0035.** The final discovery scheme uses three
> precedence-ordered roots. The single-directory sketch below is
> preserved as design history.

```
<app_data_dir>/plugins/
├── calculator.torchsnap
└── hello-world.torchsnap
```

At startup, the host scans each of the three search roots (resource-
bundled System, repo-relative Dev in debug builds, user-installed
User), reads each archive's `manifest.toml`, validates the path
guard, and registers the plugin with the `PluginHost` tagged with its
`PluginSourceKind`. The data dir for host-managed per-plugin state
lives at `<app_data_dir>/plugin-home/<plugin-id>/` — separate from
`<app_data_dir>/plugins/` which carries only plugin code.

### 6.2 WASM Plugin Adapter

A `WasmPlugin` struct wraps a wasmtime component instance. Whether it
implements the existing `Plugin` trait directly or the `PluginHost` is
refactored to accommodate WASM plugins more naturally is an open design
question — **PluginHost modifications are explicitly on the table** if they
simplify the WASM integration or improve the overall architecture.

```
PluginHost
├── NativePlugin (app-launcher)       ← compiled in
├── NativePlugin (system-commands)    ← compiled in
├── WasmPlugin (emoji-picker)         ← wraps wasmtime instance
├── WasmPlugin (calculator)           ← wraps wasmtime instance
└── ...
```

The `WasmPlugin` adapter translates each plugin method call into the
corresponding WASM guest export call, and implements the WIT host imports by
delegating to the real Tauri APIs.

### 6.3 Frontend Asset Loading

When a `.torchsnap` archive is loaded:
1. Extract `frontend/` contents to `$APPDATA/torchsnap/plugin-cache/<id>/`
2. Register the extracted JS paths in the plugin registry
3. Frontend loads them via `asset://` protocol (already scoped to `$APPDATA`)
4. The dynamic `import()` factory in `pluginComponent.tsx` gets the asset URL

**Open question:** Cache invalidation. Compare archive mtime or a hash in the
manifest against the extracted cache. Only re-extract on change.

## 7. Frontend Plugin SDK

### 7.1 Plugin Author Build Pipeline

Plugin authors need a way to write React/TypeScript components that:
- Import from `@torchsnap/keybindings`, `@torchsnap/components`, `@torchsnap/types`
- Get bundled into a single ES module per view component
- Externalize the SDK imports (they're provided by the host app at runtime)

This calls for a **plugin template project** (Rust-based) with:
- A `cargo-component` Rust crate for the WASM backend
- WIT bindings auto-generated from the torchsnap plugin WIT definitions
- A `tsconfig.json` pointing at SDK type declarations
- A Vite/rolldown config that bundles each view into a standalone ES module with
  externalized SDK imports
- A build script that produces the final `.torchsnap` zip

The official template targets **Rust** as the primary plugin language. Plugin
authors using other WASM-targeting languages (Go, C, etc.) can write their own
build tooling — the `.torchsnap` archive format and WIT contract are the
stable interface, not the build system.

### 7.2 SDK Package

The SDK types and shared components would be extracted into a publishable
package (or a local workspace package). The host app and plugin template both
depend on it. This ensures type compatibility.

```
@torchsnap/plugin-sdk
├── types.ts         # SourcedEntry, ActionId, FooterState, etc.
├── components/      # Shared UI components plugins can use
├── keybindings/     # Keybinding engine
└── hooks/           # usePluginStream, etc.
```

**Open question (deferred):** How are SDK externals resolved at runtime?
Options:
- Importmap in the HTML (e.g., `"@torchsnap/types": "/sdk/types.js"`)
- Global variable injection (less clean)
- Shared chunk that Vite already produces (current approach for built-in plugins)

Decision deferred until we reach the frontend integration phase — needs hands-on
experimentation to find the cleanest approach.

## 8. Migration Strategy

### 8.1 Parallel Execution

The `PluginHost` already works with `Box<dyn Plugin>`. Native and WASM plugins
coexist naturally — the host doesn't care about the backing implementation.

### 8.2 Conversion Order

Start with a hello-world proof-of-concept, then convert real plugins simplest-first:

| Phase | Plugin | Complexity | Why this order |
|---|---|---|---|
| 0 | **hello-world** (new) | Minimal | Pure validation of the end-to-end pipeline: WIT definition, wasmtime loading, `.torchsnap` archive parsing, manifest reading, `WasmPlugin` adapter, and basic catalog search. No host capabilities needed beyond logging. Proves the infrastructure works before touching any real plugin. |
| 1 | `commands` (built-in) | Trivial | Almost no host deps — but uses `app.exit()` and `show_settings_window()`, which are host-internal. **Skip — keep native.** |
| 1 | `calculator` | Low | Self-contained math eval, SQL history. Good first candidate: exercises SQL storage, clipboard write, settings, and CustomUI + InlineUI. |
| 2 | `emoji` | Low-medium | Embedded data (compile-time `include_str!`), nucleo matching, frecency. Exercises data embedding in WASM, frecency host API. |
| 3 | `open-url` | Medium | URL detection logic, metadata service, always-on query. Exercises HTTP/metadata host imports. |
| 3 | `bangs` | Medium | Network fetch, SQL storage, URL encoding. Similar capability profile to open-url. |
| 4 | `clipboard` | High | Most complex: clipboard watching, dual storage (SQL + blob), streaming `handle_message`, platform-specific paste, large data handling. |
| 5 | `app-launcher` | High | Platform-specific app discovery, icon extraction, background refresh. Heavy platform abstraction dependency. |
| 5 | `system-preferences` | High | Platform-specific settings pane discovery, SF Symbol icons. |
| 6 | `system-commands` | Special | Entirely platform-specific (osascript, pmset). **May stay native** — the value of WASMifying shell command execution is debatable. |

### 8.3 Step-by-Step Process for Each Plugin

1. **Define/extend WIT interfaces** needed by this plugin
2. **Implement host-side imports** in the `WasmPlugin` adapter
3. **Create the WASM guest crate** (cargo-component project)
4. **Port the plugin logic** from the native implementation
5. **Bundle frontend assets** into the `.tsplugin` archive
6. **Integration test** — both native and WASM versions active, verify identical behavior
7. **Remove the native implementation** once the WASM version is validated
8. **Clean up** — remove any host code that was only needed by the native version

### 8.4 Project Layout

The host (`src-tauri/`) is an independent cargo crate. Plugin crates live
under `plugins/` inside a virtual cargo workspace (`plugins/Cargo.toml`),
with `wasm32-wasip2` as the workspace-default target
(`plugins/.cargo/config.toml`). Host and plugins still never build in one
invocation; build orchestration is handled by just recipes.

WIT definitions live inside the `torchsnap-plugin-sdk` crate
(`plugins/plugin-sdk/wit/`) so plugin crates pick them up through the
SDK and the host references them by relative path from `src-tauri/`.

```
torchsnap/
├── src-tauri/                      # standalone Rust crate (Tauri host app)
│   ├── Cargo.toml
│   └── Cargo.lock
├── plugins/                        # virtual cargo workspace
│   ├── Cargo.toml                  # workspace manifest + release profile
│   ├── Cargo.lock                  # single lockfile for all plugins
│   ├── .cargo/config.toml          # default target = wasm32-wasip2
│   ├── plugin-sdk/                 # torchsnap-plugin-sdk crate
│   │   ├── Cargo.toml
│   │   ├── wit/
│   │   │   └── torchsnap-plugin.wit
│   │   └── src/lib.rs
│   └── hello-world/                # example plugin crate
│       ├── Cargo.toml
│       ├── manifest.toml           # plugin manifest (used by DirectorySource)
│       └── src/lib.rs
├── packages/
│   └── plugin-sdk/                 # TypeScript SDK (`@torchsnap/plugin-sdk`)
├── src/                            # frontend (unchanged)
├── package.json
└── ...
```

The WIT file is referenced by relative path from both sides:
- Host: `wasmtime::component::bindgen!({ path: "../plugins/plugin-sdk/wit" })`
- SDK crate: `wit_bindgen::generate!({ path: "wit", ... })` (relative to
  the SDK crate root); individual plugin crates don't invoke
  `generate!` themselves — they consume the bindings through
  `torchsnap_plugin_sdk::prelude::*` + `define_plugin!`.

Plugin crates during initial development live in `plugins/`. The template for
external plugins will be a standalone repo later.

### 8.5 Infrastructure Milestones

These milestones build on each other. The hello-world plugin (Phase 0) is the
forcing function that proves each piece works end-to-end.

1. **Minimal WIT definition** — define the bare-minimum `torchsnap:plugin` world
   - Lifecycle exports: `enable`, `disable`
   - Search exports: `entries`, `execute`
   - Host imports: `logging` only (simplest possible host function, proves
     bidirectional communication: host→guest calls AND guest→host calls)

2. **Hello-world guest crate** — a cargo-component Rust project in `plugins/`
   - Implements the WIT world
   - Calls `log(info, "Hello from WASM!")` in `enable()`
   - Returns one hardcoded catalog entry from `entries()`
   - Logs and returns `Dismiss` from `execute()`
   - Validates that `cargo component build` produces a valid `.wasm` component

3. **WASM host runtime** — `wasmtime` integration in the Tauri app
   - Add `wasmtime` dependency with `component-model` feature
   - `wasmtime::component::bindgen!` against the WIT definitions
   - `WasmPluginInstance` struct wrapping `Store` + typed component instance
   - `Mutex<Store>` for initial threading (revisited later)
   - One shared `wasmtime::Engine` for all WASM plugins (stored in `PluginHost`
     or Tauri state)

4. **WasmPlugin adapter** — bridges WASM instance to the plugin system
   - Implements enough of the plugin interface for catalog search + execute
   - Implements the `logging` host import (maps to `tracing::info!`)
   - Reads WASM bytes via `PluginSource::read_wasm()`

5. **Integration** — hello-world shows up in search results
   - Load hello-world via `DirectorySource` (development mode)
   - Register `WasmPlugin` with `PluginHost`
   - Verify it appears in search, execute works, log output visible

6. **Archive loader** — `.torchsnap` zip reading (`ArchiveSource`)
   - Plugin directory scanning at `$APPDATA/torchsnap/plugins/`
   - WASM binary extraction from manifest-declared path
   - Frontend asset extraction to cache directory (deferred until needed)

7. **Plugin template** — formalize the hello-world into a reusable template
   - cargo-component project structure
   - WIT bindings, build scripts
   - Archive packaging script (produces `.torchsnap`)
   - Future: add frontend Vite config when frontend plugins are tackled

8. **Frontend dynamic loading** (deferred until a real plugin with UI is converted)
   - Asset protocol serving of extracted plugin JS
   - Dynamic `import()` based on manifest paths
   - SDK externalization

## 9. WASM Store Threading Model

WASM component instances are **stateful** — linear memory persists across calls,
so a plugin can build up state in `enable()` and use it in `entries()`/`execute()`
naturally. However, wasmtime's `Store` is `!Send + !Sync`, meaning a single
store cannot be called from multiple threads concurrently.

The current native `Plugin` trait requires `Send + Sync`, and the host calls
into plugins from arbitrary tokio/blocking threads. The `WasmPluginBridge`
adapter wraps the store in a `Mutex` to satisfy this constraint.

**Decision: `Mutex<Store>`** — this is the correct and permanent model, not a
compromise. WASM execution is inherently single-threaded per instance: only one
guest function can execute at a time regardless of how host-side access is
serialized. A dedicated-thread-per-plugin model (channel + message queue) was
considered but adds complexity for no concurrency benefit — calls are serialized
either way. The Mutex is simpler, equally correct, and has no performance
disadvantage.

Cross-plugin parallelism is not affected: each plugin has its own `Store` with
its own `Mutex`, so plugin A's `search()` cannot block plugin B.

## 10. Risks and Considerations


### 9.1 Binary Size

wasmtime is a significant dependency. Expect 5-15 MB increase in the final
binary. The project already includes rusqlite (bundled SQLite) and reqwest, so
it's not unprecedented, but worth measuring early.

### 9.2 Startup Time

WASM compilation on first load can be slow. Wasmtime supports ahead-of-time
compilation to native code (`Engine::precompile_component`), producing a
`.cwasm` file. The host can cache this alongside the `.tsplugin` archive and
only recompile on archive change.

### 9.3 Performance

WASM function calls have overhead compared to native Rust. For search (called
on every keystroke), this matters. Mitigations:
- Catalog plugins: `entries()` is called frequently but returns cached data —
  the host can cache the result and only re-call on a signal
- Query plugins: `search()` already runs on `spawn_blocking` — WASM overhead
  is amortized within the blocking task
- Measure before optimizing

### 9.4 Platform-Specific Plugins

`system-commands`, `app-launcher`, and `system-preferences` are deeply
platform-specific. Two options:
- Keep them native permanently (pragmatic)
- Expose platform capabilities as host imports and let them run in WASM
  (architecturally pure but high effort for low benefit)

Recommendation: keep these native. The primary value of WASM plugins is for
**third-party extensibility** — first-party platform integrations don't benefit
from sandboxing or cross-platform distribution.

### 9.5 Debugging

WASM debugging is harder than native Rust. Plugin authors will need:
- Good error messages from host functions (not just "trap")
- `torchsnap:host/logging` for printf-style debugging
- Potential DWARF debug info in development builds

## 11. Decision Log

| Date | Decision | Rationale |
|---|---|---|
| (prior) | WIT + Component Model via wasmtime | Type safety, versioned interfaces, multi-language support |
| 2026-04-03 | Archive extension: `.torchsnap` | Brand-aligned, unambiguous. `.torch` considered but `.torchsnap` is clearer. |
| 2026-04-03 | WASM filename declared in manifest, not hardcoded | Flexibility for plugin authors; avoids naming conflicts in multi-crate setups |
| 2026-04-03 | Settings reactivity via host→guest callback | `on-setting-changed(key, value)` guest export. Simple, no polling, fits WASM call-in model. |
| 2026-04-03 | Start with hello-world plugin before converting real plugins | Validates entire pipeline (WIT, wasmtime, archive, adapter) with zero complexity. De-risks infrastructure before real migration. |
| 2026-04-03 | Official plugin template targets Rust | Most mature WIT/Component Model tooling. Other languages can target the `.torchsnap` format independently. |
| 2026-04-03 | PluginHost modifications are on the table | If refactoring PluginHost makes WASM integration cleaner, we do it rather than force-fitting into the existing API. |
| 2026-04-03 | Enable/disable is a host-level concern | Every plugin gets host-managed enable/disable. Plugin provides `on-enable`/`on-disable` callbacks. No `enabled` in plugin settings. |
| 2026-04-03 | Shortcuts are host-managed storage | Manifest declares defaults; host stores actual bindings. Plugin only receives `handle-shortcut(id)`. |
| 2026-04-03 | Settings section is flat key-value defaults | No `defaults` sub-key. The `[settings]` section IS the defaults map. |
| 2026-04-03 | Two frontend bundles per plugin (launcher + settings) | Separate webviews need separate bundles to avoid cross-window side effects and unnecessary code loading. |
| 2026-04-03 | Disabled plugins skip WASM instantiation | Only manifest is parsed. Full instantiation on enable, full teardown + drop on disable. Saves memory and startup time. |
| 2026-04-03 | Plugin metadata in manifest for settings UI | `name`, `description`, `icon` in `[plugin]` — host renders settings sidebar without plugin frontend code. |
| 2026-04-03 | No search mode field | Plugins implement full export surface; empty returns for unused modes. Matches native trait design. |
| 2026-04-03 | No cargo workspace — independent projects | Host and plugins have separate targets; build orchestration via just recipes. WIT files shared by path in `wit/` at repo root. (Superseded: plugins now share a virtual cargo workspace at `plugins/Cargo.toml`, and the WIT file lives under `plugins/plugin-sdk/wit/`.) |
| 2026-04-03 | `logging` as first host import | Simplest host function, proves bidirectional communication in hello-world |
| 2026-04-03 | `Mutex<Store>` as permanent threading model | WASM is inherently single-threaded per instance; dedicated-thread model adds complexity for no concurrency benefit. |
| 2026-04-03 | One shared `wasmtime::Engine` | All WASM plugins share an engine instance — saves compilation cache and memory |
| 2026-04-03 | Drop `cargo-component`, use `wit-bindgen` + `cargo build --target wasm32-wasip2` | cargo-component 0.21.1 pins wit-bindgen to 0.41 internally, causing version skew. Direct `wit-bindgen 0.54` + standard cargo is simpler, latest features, no extra subcommand. |
| 2026-04-03 | Call-return search model, no streaming callbacks | All current plugins send exactly one result batch per search(). Drop ResultChannel/mpsc in favor of direct return values. Simplify native Plugin trait to match. |
| 2026-04-05 | Which plugins stay native | `commands`, `system_commands`, `app_launcher`, `system_preferences` — deep platform APIs or host operations |
| 2026-04-05 | Host capabilities added on-demand | Build WIT imports as each plugin is migrated, not all upfront |
| 2026-04-05 | Calculator is first migration target | Exercises SQL + clipboard + settings — representative slice of capabilities |
| TBD | Frontend SDK externalization strategy | Deferred until frontend plugin integration phase |

## 12. Implementation Status

> Last updated: 2026-04-05

### Done

| Component | Location | Notes |
|---|---|---|
| WIT contract | `plugins/plugin-sdk/wit/torchsnap-plugin.wit` | `logging` + `types` (host imports), `lifecycle` + `search` (guest exports) |
| WASM runtime | `src-tauri/src/wasm/runtime.rs` | `WasmRuntime` (shared `Engine` + compiled-`Component` cache, split `compile()`/`instantiate()` API) + `WasmPluginInstance` (`Mutex<Store>`) |
| WasmPluginBridge | `src-tauri/src/wasm/bridge.rs` | Host-side representation of a WASM plugin. Owns the compile-at-load / instantiate-on-enable / drop-on-disable lifecycle and implements the native `Plugin` trait. |
| Plugin sources | `src-tauri/src/wasm/source.rs` | `DirectorySource` (dev) + `ArchiveSource` (production) |
| Asset protocol | `src-tauri/src/wasm/protocol.rs` | `torchsnap-plugin://localhost/<id>/<path>` with CORS + content-type |
| Manifest parsing | `src-tauri/src/wasm/manifest.rs` | Full `manifest.toml` validation with cross-field checks |
| Structured logging | `src-tauri/src/wasm/logging/` | Channel, spans, ring-buffer storage, DevTools commands |
| Type conversions | `src-tauri/src/wasm/bindings.rs` | WIT ↔ native type `From` impls |
| Hello-world plugin | `plugins/hello-world/` | Catalog + query search + custom frontend view + spans |
| Build tooling | `just/plugins.just` | `build-plugin`, `check-plugin`, `package-plugin`, `check-wit`, `fmt-wit` |
| Plugin trait refactor | `src-tauri/src/plugins/mod.rs` | `enable()`/`disable()`/`setting_changed()` lifecycle |
| Host-managed lifecycle | `src-tauri/src/plugin_host.rs` | `PluginSlot` with `AtomicBool` + `CoalescingDispatcher` |
| Frontend dynamic loading | `src/plugins/wasmPluginLoader.ts` | `initPluginSdk()`, manifest-driven registration, CSS scoping |

### In Progress

- Plugin template directory with frontend build pipeline (Vite + TSX + Tailwind)
- Plugin SDK shims for host-provided dependencies (`react`, `@torchsnap/*`)

### Not Yet Implemented — Host Capability Imports

| Capability | WIT interface | Needed by |
|---|---|---|
| SQL storage | `torchsnap:storage/sql` | calculator, bangs, clipboard |
| Blob storage | `torchsnap:storage/blob` | clipboard |
| Clipboard read/write | `torchsnap:platform/clipboard` | calculator, emoji, bangs, open-url, clipboard |
| Open URL in browser | `torchsnap:platform/opener` | bangs, open-url |
| HTTP GET/POST | `torchsnap:network/http` | bangs |
| Website metadata | `torchsnap:network/metadata` | bangs, open-url |
| Settings read | `torchsnap:settings/read` | all with settings |
| Settings watch | `torchsnap:settings/watch` | calculator, clipboard, bangs |
| Frecency scoring | `torchsnap:frecency` | emoji |

These will be added on-demand as each plugin is migrated (decision 2026-04-05).

### What Stays Native

- **`commands`** — calls `app.exit()`, window management (host operations)
- **`system_commands`** — subprocess execution (platform-specific, security-sensitive)
- **`app_launcher`** — AppKit app discovery + icon extraction (deep macOS API)
- **`system_preferences`** — plist parsing, macOS System Settings integration

### Next Milestone

Calculator plugin migration — requires `torchsnap:storage/sql`,
`torchsnap:platform/clipboard`, and `torchsnap:settings/read` host imports.
