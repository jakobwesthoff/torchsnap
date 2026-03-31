# Surface Silent CustomUI Downgrade

In `process_plugin_response()` (`plugin_host.rs:640-642`), when a query
plugin sends `PluginResponse::CustomUI` in non-prefix mode, the custom UI
is silently downgraded to plain results. No warning, no error, no log message.

A plugin author sending `CustomUI` without a prefix will get no custom UI and
no feedback about why.

Options:
- Log a warning via `tracing::warn!` or `eprintln!` (consistent with the
  inline slot contention warning)
- Return an error to the plugin via the `ResultChannel`
- Document the restriction in the `QueryPlugin` trait definition
- All of the above
