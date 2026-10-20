---
title: "Move Tauri commands into inline plugins for enforced namespacing"
kind: refactor
component: backend
tags: [api-design]
---
# Move Tauri commands into inline plugins for enforced namespacing

Commands use a flat `<group>_<name>` convention (`search`, `search_execute`, `gadget_message`, `control_subscribe` and about 40 more across `lib.rs`, `welcome.rs`, `updates`, `gadget_install` and `wasm/logging/commands.rs`). As the app grows, this is hard to navigate and creates collision risk. The command rename itself was already done as part of the control channel PR. What remains is moving the commands into Tauri inline plugins, so the namespacing is enforced by structure. The scoped naming convention for events emitted from Rust to the frontend is a separate, independent todo.

## Problem

The flat convention has no structural enforcement. Nothing prevents a new command from breaking the convention, and the frontend call site gives no visual indication of module boundaries:

```typescript
invoke("search", { ... })
invoke("gadget_message", { ... })
```

## Why not custom separators?

The `#[tauri::command]` macro uses `stringify!(fn_name)` to produce the match arm in the generated handler. Command names are therefore locked to valid Rust identifiers (alphanumeric plus `_`). Colons, pipes, dots and similar characters are impossible without bypassing the macro entirely. That would lose type-safe argument deserialization, ACL integration and the rest of the `#[tauri::command]` ergonomics.

A fully custom `invoke_handler` closure can match arbitrary strings, but it requires manual JSON parsing of `invoke.message.payload` and bypasses the ACL system. Not worth it.

## Solution: inline plugins

Tauri v2 supports **inline plugins**: plugins defined inside the app crate itself, without a separate crate. The plugin dispatch layer in Tauri routes any `invoke("plugin:<name>|<command>")` call to the matching plugin's handler. (The earlier text of this todo said "gadget" throughout, a leftover of the ADR 0042 rename. Tauri has no gadget concept; the names below were checked in the tauri 2.11.6 and tauri-build 2.6.3 sources.) This gives real, enforced namespacing with full `#[tauri::command]` macro support.

### How the dispatch works

In `tauri/src/webview/mod.rs`, the invoke routing is:

```
if cmd starts with "plugin:" → split on '|' → route to that plugin's invoke_handler
else → route to app-level invoke_handler
```

So `invoke("plugin:search|query")` routes to the `search` plugin's handler, which matches the bare command name `query`. The `plugin:` prefix and the `|` separator are hard-coded in Tauri's dispatch, so no custom configuration is needed.

### Implementation pattern

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

Registration in `main.rs` or `lib.rs`:

```rust
tauri::Builder::default()
    .plugin(commands::search::init())
    .plugin(commands::control::init())
    .plugin(commands::gadget_host::init())
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

### ACL and permissions

Each inline plugin gets its own permission scope. Tauri is expected to generate a `default` permission per plugin from the commands listed in `build.rs`; the permission names in this section (`<plugin>:default`, `<plugin>:allow-<command>`) are not yet checked against the Tauri 2 ACL docs. These must be referenced in `src-tauri/capabilities/*.json`:

```json
{
  "permissions": [
    "search:default",
    "control:default",
    "gadget-host:default"
  ]
}
```

For fine-grained control, individual command permissions follow the pattern `<plugin-name>:allow-<command>` or `<plugin-name>:deny-<command>`.

### Migration checklist

For each command group (search, control, gadget, clipboard, settings, and so on):

1. Create a module under `src-tauri/src/commands/<group>.rs`, or keep the existing module structure. The key change is the `init()` function returning a `TauriPlugin`.
2. Move `#[tauri::command]` functions into that module. Strip the group prefix from function names (`search` becomes `query`, `gadget_message` becomes `message`).
3. Add `pub fn init<R: Runtime>() -> TauriGadget<R>` that builds the plugin with `Builder::new("<group>").invoke_handler(generate_handler![...]).build()`.
4. Register the plugin in the Tauri builder with `.plugin(commands::group::init())`.
5. Remove the commands from the app-level `generate_handler![]`.
6. Add the plugin to `build.rs` via `tauri_build::InlinedPlugin`.
7. Add `<group>:default` (or per-command permissions) to the capabilities JSON.
8. Update all frontend `invoke()` calls from `invoke("group_command")` to `invoke("plugin:group|command")`.
9. Update any TypeScript type bindings if they are auto-generated.

### State access

Inline plugins can use `.setup(|app, _api| { ... })` to initialize plugin-local state. They can also access app-wide `State<T>` in commands the same way as regular commands, since `#[tauri::command]` injection works identically inside plugins.

### Caveats

- The `plugin:` prefix in frontend calls is a fixed Tauri convention. It cannot be customized (for example `app:search|query` is not possible without patching Tauri).
- Each plugin namespace requires a `build.rs` entry. This is mechanical but must not be forgotten. Missing entries cause silent ACL denials.
- Plugin names use `-` as the word separator by convention (`gadget-host`, not `gadget_host`), but this is a convention, not a hard constraint.
