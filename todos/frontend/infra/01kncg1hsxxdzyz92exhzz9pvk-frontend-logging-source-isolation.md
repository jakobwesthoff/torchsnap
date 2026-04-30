# Plugin Frontend API Isolation

## Problem

Plugin frontends run in webviews and have access to the full Tauri IPC
surface. Any plugin can call any registered Tauri command — `search_execute`,
`devtools_log_clear`, `frecency_clear`, etc. A misbehaving plugin frontend
could:

- Impersonate another plugin's log source
- Clear the log buffer or frecency data
- Invoke commands meant for the host app (settings, tray, window management)
- Call `plugin_message` with another plugin's source ID
- Access any Tauri capability granted to the window

This is the general case of the logging source isolation problem. The logging
API (`devtools_log_emit`, `devtools_span_start`, `devtools_span_end`) accepts
a `source` string that any caller can fake, but the same trust problem applies
to every command that takes a plugin ID or affects shared state.

## Current Design

Trust-based. Plugin frontends receive pre-configured utilities via
props/context (e.g., a logger with baked-in plugin ID, a command helper
scoped to the plugin). No backend enforcement.

## Isolation Strategies

### 1. Tauri Capabilities (per-window permissions)

Tauri's capability system can restrict which commands a window can invoke.
Each plugin webview gets a capability file that only allows the commands the
plugin needs (e.g., `plugin_message`, `devtools_log_emit`) and denies
everything else. This is the most Tauri-native approach.

Limitation: capabilities are static — defined at build time per window label
pattern. Dynamic plugin loading would need a convention for generating
capability files.

### 2. Window Label Validation

Commands that accept a source/plugin ID parameter validate it against the
calling window's label. Tauri commands receive the window handle, so:

```rust
#[tauri::command]
fn devtools_log_emit(window: tauri::Window, source: String, ...) {
    // window.label() is e.g. "plugin-hello-world-launcher"
    // Validate that `source` matches the plugin ID in the label.
}
```

This handles identity spoofing but not unauthorized command access.

### 3. Plugin IPC Proxy

Instead of giving plugin webviews direct access to Tauri commands, route all
plugin IPC through a single `plugin_ipc` command that validates the caller
and dispatches to the appropriate handler. The plugin frontend only sees a
restricted API surface.

```rust
#[tauri::command]
fn plugin_ipc(window: tauri::Window, action: PluginAction) -> Result<...> {
    let plugin_id = extract_plugin_id(window.label())?;
    match action {
        PluginAction::Log { level, message, .. } => { /* source = plugin_id */ }
        PluginAction::SpanStart { .. } => { ... }
        PluginAction::Message { method, payload } => { ... }
        // Only actions plugins are allowed to perform
    }
}
```

This is the strongest isolation — plugins never see raw Tauri commands.

### 4. Combination

Use Tauri capabilities (strategy 1) to restrict which commands a plugin window
can call, plus window label validation (strategy 2) for commands that need
identity verification. This gives defense in depth without requiring a full
proxy layer.

## Decision

Deferred. Trust-based is acceptable while plugins are first-party or
user-installed. Revisit when the plugin ecosystem opens to third parties.
Strategy 4 (capabilities + label validation) is the likely path — it's
incremental and uses existing Tauri infrastructure.

## Related

- Frontend logging commands (`devtools_log_emit`, `devtools_span_start`,
  `devtools_span_end`) are the first commands where source isolation matters
  in practice.
- `plugin_message` already takes a `source` parameter that could be faked.
- Tauri capability files live in `src-tauri/capabilities/`.
