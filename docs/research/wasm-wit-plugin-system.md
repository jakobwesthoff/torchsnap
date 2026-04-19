# WASM Plugin System via WIT + Component Model

## Decision

Use the **WIT (Wasm Interface Type) + Component Model** approach over extism or
raw wasip1 for the Torchsnap plugin system. The type-safe, versioned contract
across the host/guest boundary justifies the upfront investment.

## Why Not Alternatives

### WASI Preview 1 (wasip1)
- Flat C-like fd-based API, no typed interfaces
- No component model — you'd reinvent an ad-hoc ABI over shared memory or stdio
- No async, no sockets (only files/stdio)

### Extism
- Higher-level framework, fast to prototype with
- Plugin boundary is `fn(bytes) -> bytes` — JSON serialization, no compile-time
  type safety across host/guest
- As the plugin API grows (new host capabilities, event types), the lack of
  typed contracts becomes painful — you end up building your own schema
  validation layer, reimplementing what WIT gives for free

### WASI Preview 2 + WIT (chosen)
- Built on the Component Model — the key architectural shift
- WIT definitions provide typed records, enums, variants across languages
- Versioned interfaces with explicit API evolution
- Strong compile-time guarantees: a WIT change breaks compilation on both sides,
  no silent JSON mismatches

## WIT Maturity (as of early 2026)

| Area | Status |
|---|---|
| WIT syntax & semantics | Stable, spec finalized |
| wasmtime component support | Production-grade (stable since wasmtime 14, late 2023) |
| `wit-bindgen` (code generation) | Stable for Rust and Go, functional for others |
| WASI interfaces defined in WIT | wasip2 interfaces standardized; higher-level proposals (keyvalue, messaging) still draft |
| Composition tooling (wac, wasm-tools compose) | Works but still evolving |

## Language Support for Plugin Authors

| Language | Guest (write plugins) | Host (load plugins) | Maturity |
|---|---|---|---|
| **Rust** | `cargo-component`, `wit-bindgen` | `wasmtime::component::bindgen!` | Most mature |
| **Go** | `wit-bindgen-go`, TinyGo target | `wazero` + bindings | Solid |
| **Python** | `componentize-py` | wasmtime-py | Works, some friction |
| **JavaScript** | `componentize-js` (via jco) | `jco` for Node/Deno | Actively developed |
| **C/C++** | `wit-bindgen-c` | wasmtime C API | Bare-bones |
| **C#/.NET** | `componentize-dotnet` | wasmtime dotnet | Early/experimental |
| **Kotlin/Java** | Experimental | wasmtime JVM bindings | Early/experimental |
| **Zig** | `wit-bindgen-zig` (community) | Not directly | Community-maintained |

Rust and Go are comfortable choices for plugin authors today. JS and Python
work but the DX is rougher.

## Key Toolchain

- **`cargo-component`** — Cargo subcommand for building Wasm components from
  Rust. Handles wit-bindgen automatically.
- **`wit-bindgen`** — generates language-specific bindings from WIT files.
  Usually invoked indirectly through cargo-component or build plugins.
- **`wasm-tools`** — Swiss army knife for inspecting, validating, and composing
  components.
- **`jco`** — JavaScript Component toolchain (transpile components to JS, or
  componentize JS into Wasm).

## Architecture Sketch

### 1. Define the plugin contract in WIT

```wit
package torchsnap:plugin@0.1.0;

interface host {
    record event {
        kind: event-kind,
        timestamp: u64,
        payload: option<string>,
    }

    enum event-kind {
        focus-changed,
        clipboard-update,
        timer-fired,
    }

    enum action {
        none,
        show-notification,
        update-tray,
    }

    record notification {
        title: string,
        body: option<string>,
    }

    log: func(level: log-level, msg: string);
    show-notification: func(n: notification);
    get-config: func(key: string) -> option<string>;

    enum log-level {
        debug,
        info,
        warn,
        error,
    }
}

world plugin {
    import host;

    export init: func() -> result<_, string>;
    export handle-event: func(e: host.event) -> list<host.action>;
    export name: func() -> string;
    export version: func() -> string;
}
```

### 2. Guest (plugin author) implements against the WIT

```rust
cargo_component_bindings::generate!();

use bindings::exports::torchsnap::plugin::*;
use bindings::torchsnap::plugin::host;

struct MyPlugin;

impl Guest for MyPlugin {
    fn init() -> Result<(), String> {
        host::log(host::LogLevel::Info, "plugin loaded");
        Ok(())
    }

    fn name() -> String {
        "clipboard-manager".into()
    }

    fn version() -> String {
        "0.1.0".into()
    }

    fn handle_event(e: host::Event) -> Vec<host::Action> {
        match e.kind {
            host::EventKind::ClipboardUpdate => {
                host::show_notification(host::Notification {
                    title: "Clipboard".into(),
                    body: e.payload,
                });
                vec![host::Action::ShowNotification]
            }
            _ => vec![host::Action::None],
        }
    }
}
```

Build with `cargo component build --release` — produces a `.wasm` component.

### 3. Host loads with full type safety

```rust
use wasmtime::component::*;
use wasmtime::{Config, Engine, Store};

wasmtime::component::bindgen!("plugin" in "plugins/plugin-sdk/wit/");

struct HostState { /* app state */ }

impl torchsnap::plugin::host::Host for HostState {
    fn log(&mut self, level: LogLevel, msg: String) {
        tracing::info!(%msg, ?level, "plugin");
    }

    fn show_notification(&mut self, n: Notification) {
        // wire to Tauri notification API
    }

    fn get_config(&mut self, key: String) -> Option<String> {
        self.config.get(&key).cloned()
    }
}

let engine = Engine::new(Config::new().wasm_component_model(true))?;
let component = Component::from_file(&engine, "plugins/clipboard.wasm")?;

let mut linker = Linker::new(&engine);
Plugin::add_to_linker(&mut linker, |state: &mut HostState| state)?;

let mut store = Store::new(&engine, HostState { /* ... */ });
let (plugin, _) = Plugin::instantiate(&mut store, &component, &linker)?;

let name = plugin.call_name(&mut store)?;
let actions = plugin.call_handle_event(&mut store, &Event {
    kind: EventKind::ClipboardUpdate,
    timestamp: 1234567890,
    payload: Some("copied text".into()),
})?;
```

## Open Design Questions

- **Which Tauri APIs to expose via WIT?** Plugins need access to windows,
  system tray, notifications, clipboard, etc. None of this is in standard WASI —
  custom WIT interfaces that proxy through the host are needed. This is the main
  design work.
- **Plugin sandboxing granularity** — per-plugin capability grants (e.g., plugin
  X gets clipboard access but not filesystem)?
- **Async** — how to handle async host functions (e.g., HTTP requests from
  plugins)? WASI async proposals are still in flux.
- **Plugin discovery and distribution** — registry, local directory scanning, or
  both?
