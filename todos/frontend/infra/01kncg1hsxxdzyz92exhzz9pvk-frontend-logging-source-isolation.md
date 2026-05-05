# Gadget Frontend API Isolation

## Problem

Gadget frontends run in webviews and have access to the full Tauri IPC
surface. Any gadget can call any registered Tauri command — `search_execute`,
`devtools_log_clear`, `frecency_clear`, etc. A misbehaving gadget frontend
could:

- Impersonate another gadget's log source
- Clear the log buffer or frecency data
- Invoke commands meant for the host app (settings, tray, window management)
- Call `gadget_message` with another gadget's source ID
- Access any Tauri capability granted to the window

This is the general case of the logging source isolation problem. The logging
API (`devtools_log_emit`, `devtools_span_start`, `devtools_span_end`) accepts
a `source` string that any caller can fake, but the same trust problem applies
to every command that takes a gadget ID or affects shared state.

## Current Design

Trust-based. Gadget frontends receive pre-configured utilities via
props/context (e.g., a logger with baked-in gadget ID, a command helper
scoped to the gadget). No backend enforcement.

## Isolation Strategies

### 1. Tauri Capabilities (per-window permissions)

Tauri's capability system can restrict which commands a window can invoke.
Each gadget webview gets a capability file that only allows the commands the
gadget needs (e.g., `gadget_message`, `devtools_log_emit`) and denies
everything else. This is the most Tauri-native approach.

Limitation: capabilities are static — defined at build time per window label
pattern. Dynamic gadget loading would need a convention for generating
capability files.

### 2. Window Label Validation

Commands that accept a source/gadget ID parameter validate it against the
calling window's label. Tauri commands receive the window handle, so:

```rust
#[tauri::command]
fn devtools_log_emit(window: tauri::Window, source: String, ...) {
    // window.label() is e.g. "gadget-hello-world-launcher"
    // Validate that `source` matches the gadget ID in the label.
}
```

This handles identity spoofing but not unauthorized command access.

### 3. Gadget IPC Proxy

Instead of giving gadget webviews direct access to Tauri commands, route all
gadget IPC through a single `gadget_ipc` command that validates the caller
and dispatches to the appropriate handler. The gadget frontend only sees a
restricted API surface.

```rust
#[tauri::command]
fn gadget_ipc(window: tauri::Window, action: GadgetAction) -> Result<...> {
    let gadget_id = extract_gadget_id(window.label())?;
    match action {
        GadgetAction::Log { level, message, .. } => { /* source = gadget_id */ }
        GadgetAction::SpanStart { .. } => { ... }
        GadgetAction::Message { method, payload } => { ... }
        // Only actions gadgets are allowed to perform
    }
}
```

This is the strongest isolation — gadgets never see raw Tauri commands.

### 4. Combination

Use Tauri capabilities (strategy 1) to restrict which commands a gadget window
can call, plus window label validation (strategy 2) for commands that need
identity verification. This gives defense in depth without requiring a full
proxy layer.

## Decision

Deferred. Trust-based is acceptable while gadgets are first-party or
user-installed. Revisit when the gadget ecosystem opens to third parties.
Strategy 4 (capabilities + label validation) is the likely path — it's
incremental and uses existing Tauri infrastructure.

## Related

- Frontend logging commands (`devtools_log_emit`, `devtools_span_start`,
  `devtools_span_end`) are the first commands where source isolation matters
  in practice.
- `gadget_message` already takes a `source` parameter that could be faked.
- Tauri capability files live in `src-tauri/capabilities/`.
