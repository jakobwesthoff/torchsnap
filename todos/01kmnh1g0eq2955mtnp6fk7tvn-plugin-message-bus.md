# Plugin message bus (sendMessage / handle_message)

The plugin custom UI system (ADR 0013, topic 4) defines a message bus
for direct frontend ↔ backend communication within a plugin. This
includes:

- `handle_message(method, payload, channel) -> Result<MessageResponse>`
  on the `QueryPlugin` trait
- A single `plugin_message` Tauri command that dispatches by plugin ID
- A `sendMessage<TResult, TStream>` prop passed to the plugin component
- `MessagePayload`, `MessageResponse`, `StreamItem` newtypes over
  `serde_json::Value`

Currently stubbed: the trait method has a default implementation that
returns an error. The Tauri command and frontend wrapper are not yet
built.

Implement when the first plugin needs custom backend communication
beyond the standard search/execute flow.
