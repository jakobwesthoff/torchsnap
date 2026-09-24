---
kind: feature
status: open
---

# Namespace Tauri Commands and Events

## Context

The command rename (`search_query`, `search_execute`, `gadget_message`,
`control_subscribe`) was done as part of the control channel PR. This todo
covers two remaining improvements:

1. **Commands**: Replace the flat underscore-prefixed convention with proper
   structural namespacing using Tauri's inline gadget mechanism.
2. **Events**: Adopt a scoped naming convention for Tauri events emitted from
   Rust to the frontend.

---

## Part 1: Command Namespacing via Inline Gadgets

### Problem

Commands currently use a flat `<group>_<name>` convention (`search_query`,
`gadget_message`, `control_subscribe`). This works but has no structural
enforcement — nothing prevents a new command from breaking the convention, and
the frontend call site gives no visual indication of module boundaries:

```typescript
invoke("search_query", { ... })
invoke("gadget_message", { ... })
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

### Solution: Inline Gadgets

Tauri v2 supports **inline gadgets** — gadgets defined inside the app crate
itself, without a separate crate. The gadget dispatch layer in Tauri routes any
`invoke("gadget:<name>|<command>")` call to the matching gadget's handler. This
gives real, enforced namespacing with full `#[tauri::command]` macro support.

#### How the dispatch works

In `tauri/src/webview/mod.rs`, the invoke routing is:

```
if cmd starts with "gadget:" → split on '|' → route to that gadget's invoke_handler
else → route to app-level invoke_handler
```

This means `invoke("gadget:search|query")` routes to the `search` gadget's
handler, which matches the bare command name `query`. The `gadget:` prefix and
`|` separator are hard-coded in Tauri's dispatch — no custom configuration
needed.

#### Implementation pattern

Each namespace becomes a small module that returns a `TauriGadget`:

```rust
// src-tauri/src/commands/search.rs
use tauri::{command, generate_handler, gadget::Builder, gadget::TauriGadget, Runtime};

#[command]
async fn query(/* args */) -> Result</* ... */> {
    // ...
}

#[command]
async fn execute(/* args */) -> Result</* ... */> {
    // ...
}

pub fn init<R: Runtime>() -> TauriGadget<R> {
    Builder::new("search")
        .invoke_handler(generate_handler![query, execute])
        .build()
}
```

Registration in `main.rs` / `lib.rs`:

```rust
tauri::Builder::default()
    .gadget(commands::search::init())
    .gadget(commands::control::init())
    .gadget(commands::gadget_host::init())
    // ...
    .run(tauri::generate_context!())
```

Each inline gadget needs a corresponding entry in `build.rs` for ACL:

```rust
// build.rs
tauri_build::try_build(
    tauri_build::Attributes::new()
        .gadget(
            "search",
            tauri_build::InlinedGadget::new().commands(&["query", "execute"]),
        )
        .gadget(
            "control",
            tauri_build::InlinedGadget::new().commands(&["subscribe"]),
        )
)
```

Frontend calls become explicitly namespaced:

```typescript
invoke("gadget:search|query", { query: "..." })
invoke("gadget:search|execute", { id: "..." })
invoke("gadget:control|subscribe", { channel: "..." })
```

#### ACL / Permissions

Each inline gadget gets its own permission scope. Tauri auto-generates a
`default` permission per gadget from the commands listed in `build.rs`. These
must be referenced in `src-tauri/capabilities/*.json`:

```json
{
  "permissions": [
    "search:default",
    "control:default",
    "gadget-host:default"
  ]
}
```

If fine-grained control is needed, individual command permissions follow the
pattern `<gadget-name>:allow-<command>` / `<gadget-name>:deny-<command>`.

#### Migration checklist

For each command group (search, control, gadget, clipboard, settings, etc.):

1. Create a module under `src-tauri/src/commands/<group>.rs` (or keep existing
   module structure — the key change is the `init()` function returning a
   `TauriGadget`).
2. Move `#[tauri::command]` functions into that module. Strip the group prefix
   from function names (`search_query` → `query`, `gadget_message` → `message`).
3. Add `pub fn init<R: Runtime>() -> TauriGadget<R>` that builds the gadget
   with `Builder::new("<group>").invoke_handler(generate_handler![...]).build()`.
4. Register the gadget in the Tauri builder with `.gadget(commands::group::init())`.
5. Remove the commands from the app-level `generate_handler![]`.
6. Add the gadget to `build.rs` via `tauri_build::InlinedGadget`.
7. Add `<group>:default` (or per-command permissions) to capabilities JSON.
8. Update all frontend `invoke()` calls from `invoke("group_command")` to
   `invoke("gadget:group|command")`.
9. Update any TypeScript type bindings if auto-generated.

#### State access

Inline gadgets can use `.setup(|app, _api| { ... })` to initialize gadget-local
state, or access app-wide `State<T>` in commands the same way as regular
commands — `#[tauri::command]` injection works identically inside gadgets.

#### Caveats

- The `gadget:` prefix in frontend calls is a fixed Tauri convention. There is
  no way to customize it (e.g., `app:search|query` is not possible without
  patching Tauri).
- Each gadget namespace requires a `build.rs` entry. This is mechanical but
  must not be forgotten — missing entries cause silent ACL denials.
- Gadget names use `-` as word separator by convention (`gadget-host`, not
  `gadget_host`), but this is a convention not a hard constraint.

---

## Part 2: Event Namespacing

### Problem

Tauri events currently use flat, unscoped names (`settings-changed`,
`activate-gadget-custom-ui`, `react-ready`). As the app grows, this becomes
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
- `activate-gadget-custom-ui` → `gadget:activate-custom-ui`
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
