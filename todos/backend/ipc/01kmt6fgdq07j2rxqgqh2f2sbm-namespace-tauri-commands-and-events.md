# Namespace Tauri Commands and Events

## Context

The command rename (`search_query`, `search_execute`, `plugin_message`,
`control_subscribe`) was done as part of the control channel PR. This todo
covers two remaining improvements:

1. **Commands**: Replace the flat underscore-prefixed convention with proper
   structural namespacing using Tauri's inline plugin mechanism.
2. **Events**: Adopt a scoped naming convention for Tauri events emitted from
   Rust to the frontend.

---

## Part 1: Command Namespacing via Inline Plugins

### Problem

Commands currently use a flat `<group>_<name>` convention (`search_query`,
`plugin_message`, `control_subscribe`). This works but has no structural
enforcement — nothing prevents a new command from breaking the convention, and
the frontend call site gives no visual indication of module boundaries:

```typescript
invoke("search_query", { ... })
invoke("plugin_message", { ... })
```

### Why not custom separators?

The `#[tauri::command]` macro uses `stringify!(fn_name)` to produce the match
arm in the generated handler. Command names are therefore locked to valid Rust
identifiers (alphanumeric + `_`). Colons, pipes, dots, etc. are impossible
without bypassing the macro entirely — which would lose type-safe argument
deserialization, ACL integration, and all `#[tauri::command]` ergonomics.

A fully custom `invoke_handler` closure *can* match arbitrary strings, but
requires manual JSON parsing of `invoke.message.payload` and bypasses the ACL
system. Not worth it.

### Solution: Inline Plugins

Tauri v2 supports **inline plugins** — plugins defined inside the app crate
itself, without a separate crate. The plugin dispatch layer in Tauri routes any
`invoke("plugin:<name>|<command>")` call to the matching plugin's handler. This
gives real, enforced namespacing with full `#[tauri::command]` macro support.

#### How the dispatch works

In `tauri/src/webview/mod.rs`, the invoke routing is:

```
if cmd starts with "plugin:" → split on '|' → route to that plugin's invoke_handler
else → route to app-level invoke_handler
```

This means `invoke("plugin:search|query")` routes to the `search` plugin's
handler, which matches the bare command name `query`. The `plugin:` prefix and
`|` separator are hard-coded in Tauri's dispatch — no custom configuration
needed.

#### Implementation pattern

Each namespace becomes a small module that returns a `TauriPlugin`:

```rust
// src-tauri/src/commands/search.rs
use tauri::{command, generate_handler, plugin::Builder, plugin::TauriPlugin, Runtime};

#[command]
async fn query(/* args */) -> Result</* ... */> {
    // ...
}

#[command]
async fn execute(/* args */) -> Result</* ... */> {
    // ...
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("search")
        .invoke_handler(generate_handler![query, execute])
        .build()
}
```

Registration in `main.rs` / `lib.rs`:

```rust
tauri::Builder::default()
    .plugin(commands::search::init())
    .plugin(commands::control::init())
    .plugin(commands::plugin_host::init())
    // ...
    .run(tauri::generate_context!())
```

Each inline plugin needs a corresponding entry in `build.rs` for ACL:

```rust
// build.rs
tauri_build::try_build(
    tauri_build::Attributes::new()
        .plugin(
            "search",
            tauri_build::InlinedPlugin::new().commands(&["query", "execute"]),
        )
        .plugin(
            "control",
            tauri_build::InlinedPlugin::new().commands(&["subscribe"]),
        )
)
```

Frontend calls become explicitly namespaced:

```typescript
invoke("plugin:search|query", { query: "..." })
invoke("plugin:search|execute", { id: "..." })
invoke("plugin:control|subscribe", { channel: "..." })
```

#### ACL / Permissions

Each inline plugin gets its own permission scope. Tauri auto-generates a
`default` permission per plugin from the commands listed in `build.rs`. These
must be referenced in `src-tauri/capabilities/*.json`:

```json
{
  "permissions": [
    "search:default",
    "control:default",
    "plugin-host:default"
  ]
}
```

If fine-grained control is needed, individual command permissions follow the
pattern `<plugin-name>:allow-<command>` / `<plugin-name>:deny-<command>`.

#### Migration checklist

For each command group (search, control, plugin, clipboard, settings, etc.):

1. Create a module under `src-tauri/src/commands/<group>.rs` (or keep existing
   module structure — the key change is the `init()` function returning a
   `TauriPlugin`).
2. Move `#[tauri::command]` functions into that module. Strip the group prefix
   from function names (`search_query` → `query`, `plugin_message` → `message`).
3. Add `pub fn init<R: Runtime>() -> TauriPlugin<R>` that builds the plugin
   with `Builder::new("<group>").invoke_handler(generate_handler![...]).build()`.
4. Register the plugin in the Tauri builder with `.plugin(commands::group::init())`.
5. Remove the commands from the app-level `generate_handler![]`.
6. Add the plugin to `build.rs` via `tauri_build::InlinedPlugin`.
7. Add `<group>:default` (or per-command permissions) to capabilities JSON.
8. Update all frontend `invoke()` calls from `invoke("group_command")` to
   `invoke("plugin:group|command")`.
9. Update any TypeScript type bindings if auto-generated.

#### State access

Inline plugins can use `.setup(|app, _api| { ... })` to initialize plugin-local
state, or access app-wide `State<T>` in commands the same way as regular
commands — `#[tauri::command]` injection works identically inside plugins.

#### Caveats

- The `plugin:` prefix in frontend calls is a fixed Tauri convention. There is
  no way to customize it (e.g., `app:search|query` is not possible without
  patching Tauri).
- Each plugin namespace requires a `build.rs` entry. This is mechanical but
  must not be forgotten — missing entries cause silent ACL denials.
- Plugin names use `-` as word separator by convention (`plugin-host`, not
  `plugin_host`), but this is a convention not a hard constraint.

---

## Part 2: Event Namespacing

### Problem

Tauri events currently use flat, unscoped names (`settings-changed`,
`activate-plugin-custom-ui`, `react-ready`). As the app grows, this becomes
hard to navigate and creates collision risk.

### Proposed Convention

Since event names are arbitrary strings (no Rust identifier constraint), events
should use `:` as a separator for clarity and visual distinction from commands:

```
<namespace>:<event_name>
```

### Events (Rust → JS via `emit()`)

Current → Proposed:
- `settings-changed` → `settings:changed`
- `activate-plugin-custom-ui` → `plugin:activate-custom-ui`
- `react-ready` → `lifecycle:react-ready`

The `:` separator is preferred over `_` because:
- It visually distinguishes events from Rust identifiers and command names.
- It matches common event naming conventions in other systems (DOM events,
  Node.js EventEmitter patterns like `server:listening`).
- Events are not constrained by the `#[tauri::command]` macro, so there is no
  technical reason to limit them to `_`.

### Technical Notes

- Event names are plain strings — any separator works.
- No behavior change, pure rename.
- This is independent of Part 1 and can be done separately.

### Scope

- All `app.emit()` / `app.emit_to()` calls in Rust
- All `listen()` / `appWindow.listen()` calls in TypeScript
