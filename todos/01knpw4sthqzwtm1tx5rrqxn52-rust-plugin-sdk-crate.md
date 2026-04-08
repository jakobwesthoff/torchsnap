# Rust plugin SDK crate (`@torchsnap/plugin-sdk-rs` / `torchsnap-plugin-sdk`)

The frontend already has `@torchsnap/plugin-sdk` (TypeScript) for plugin
authors. The Rust side currently has **no** equivalent — every WASM plugin
calls the raw `wit_bindgen`-generated bindings directly and hand-rolls the
boilerplate around them. As more plugins land (calculator port, the
test-fixture crate, future user plugins), the same boilerplate is going to
appear in every `lib.rs`.

We should extract a Rust SDK crate before that happens. The crate would
live in-tree as `plugin-sdk-rs/` (workspace member, separate from the
existing TypeScript `plugin-sdk/`) and be referenced via `path = "..."`
from each plugin's `Cargo.toml`. NPM/crates.io publish stays deferred —
plugins are in-monorepo for now.

## What should be in it (current scope)

### Settings helpers (D2 / ADR 0029)

The template plugin currently does:

```rust
fn read_bool_setting(key: &str) -> Option<bool> {
    serde_json::from_str(&settings::get(key)?).ok()
}
fn read_string_setting(key: &str) -> Option<String> {
    serde_json::from_str(&settings::get(key)?).ok()
}
```

This should become:

```rust
use torchsnap_plugin_sdk::settings;

let verbose: bool = settings::get("verbose").unwrap_or(false);
let greeting: String = settings::get_or_else("greeting", || "(unset)".into());
```

with the SDK providing a single generic `get<T: DeserializeOwned>(key) -> Option<T>`
plus a `get_or` / `get_or_else` convenience pair. Internally it calls
`torchsnap::plugin::settings::get` and threads through the JSON parse.

### Messaging helpers (D3 / ADR 0030)

Plugin code currently writes:

```rust
match method.as_str() {
    "echo" => Ok(payload),
    "current-greeting" => {
        let greeting = read_string_setting("greeting").unwrap_or_default();
        serde_json::to_string(&serde_json::json!({ "greeting": greeting }))
            .map_err(|e| format!("serialize response: {e}"))
    }
    other => Err(format!("unknown method: {other}")),
}
```

Two wins available:

1. **Typed payload / response helpers**:

   ```rust
   pub fn parse_payload<T: DeserializeOwned>(payload: &str) -> Result<T, String> {
       serde_json::from_str(payload).map_err(|e| format!("invalid payload: {e}"))
   }

   pub fn to_response<T: Serialize>(value: &T) -> Result<String, String> {
       serde_json::to_string(value).map_err(|e| format!("serialize response: {e}"))
   }
   ```

   So the inner method handler shrinks to:

   ```rust
   "save_history" => {
       let req: SaveHistoryRequest = sdk::parse_payload(&payload)?;
       sdk::to_response(&SaveHistoryResponse { saved: true })
   }
   ```

2. **A `dispatch!` macro** (later, after multiple plugins motivate it):

   ```rust
   sdk::dispatch! { method, payload,
       "echo" => |p: serde_json::Value| Ok(p),
       "current-greeting" => |_: ()| compute_greeting(),
   }
   ```

   Pure ergonomic sugar. Skip until two or three plugins repeat the
   `match method.as_str()` boilerplate.

### Logging helpers (existing)

The host's `logging` interface already exposes structured logging with
spans. The SDK should re-export it under a less namespace-y path:

```rust
use torchsnap_plugin_sdk::logging::{LogLevel, log, span_start, span_end};
```

Plus a `log_error!` / `log_info!` macro family that captures source
location automatically (currently every call passes `&[]` for metadata
even when there's nothing to attach). Same pattern as `tracing::info!`.

### SQL storage helpers (D1 — coming next)

Once D1 lands, the SDK gains:

- `storage::open() -> Result<SqlHandle, String>` — thin re-export
- A `SqlValue` builder helper trait so `vec![v("name"), v(42), v(true)]`
  works without writing `SqlValue::Text("name".into())` manually
- A `query_one<T: DeserializeOwned>` / `query_all<T>` helper that
  destructures rows via column index, so plugin code can read
  `row.get::<String>(0)?` exactly like host code does today
- A `transaction(|tx| { ... })` wrapper analogous to the host's

### Scheduled tasks helpers (D4 — coming next)

Once D4 lands, the SDK gains:

- A no-op default `run-task` impl so plugins without `[[tasks]]` get
  zero boilerplate (the plan locks this in already)
- Optional macro for matching task ids by name

### Core trait wrappers / re-exports

The plugin author should ideally NOT need to know that the WIT
interface module path is `exports::torchsnap::plugin::lifecycle::Guest`.
The SDK should re-export the four guest traits under flatter names:

```rust
pub use torchsnap_plugin_sdk::{
    LifecycleGuest, SearchGuest, MessagingGuest, TasksGuest,
};
```

And ideally a single `define_plugin!` macro that combines `wit_bindgen::generate!`
+ `export!(MyPlugin)` so the boilerplate top-of-file becomes one line:

```rust
torchsnap_plugin_sdk::define_plugin!(MyPlugin);
```

## What is NOT in scope

- **Publishing to crates.io** — same deferral as `@torchsnap/plugin-sdk`
  on the TypeScript side. In-monorepo path dependency for now.
- **Replacing `wit-bindgen`** — the SDK is a thin layer ON TOP of
  bindgen, not a replacement. The macro hides the boring path imports
  but the generated types stay accessible.
- **A separate "SDK runtime"** — every helper compiles down to the same
  `wit_bindgen`-generated calls plus `serde_json`. No new dependency.

## Trigger for actually doing this work

The tipping point is the **calculator port** (task #8 in the migration
plan). The calculator's `handle_message` dispatcher has three methods,
each parsing a typed request and serializing a typed response — exactly
the boilerplate that the SDK helpers shrink. If the calculator port lands
without the SDK, the boilerplate becomes hard to retrofit without a
churn-heavy rewrite of calculator code.

Alternative trigger: when the dedicated `plugins/test-fixture/` crate
(also still pending) needs ~8 dispatch methods to exercise the bridge.
That's even more boilerplate per file than the calculator.

## Suggested PR slicing when this lands

1. Create the `plugin-sdk-rs/` crate skeleton with an empty `lib.rs`
   and add it to the workspace.
2. Move the existing settings helpers from
   `plugins/template/src/lib.rs` into the SDK.
3. Add the messaging `parse_payload` / `to_response` helpers and
   migrate the template plugin to use them.
4. Add the trait re-exports + `define_plugin!` macro.
5. Once D1 / D4 land: add the storage and tasks helpers in their own
   PRs alongside the plugins that motivate them.
