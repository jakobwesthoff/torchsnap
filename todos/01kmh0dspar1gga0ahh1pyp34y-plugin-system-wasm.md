# Plugin system using WASM

Torchsnap should support a plugin architecture where third-party
extensions can provide search results, commands, and UI to the
launcher. WASM is the primary candidate for sandboxed plugin
execution.

## Open questions to discuss

- **Plugin interface**: What does a plugin provide?
  - Search results for a query (async)
  - Static commands (always available)
  - Custom result row rendering?
  - Settings UI contributions?
- **WASM runtime**: Which runtime on the Rust side?
  - `wasmtime` (mature, WASI support)
  - `wasmer` (alternative)
  - Extism (plugin framework built on wasm)
- **Plugin ↔ host communication**:
  - How does a plugin receive the search query?
  - How does it return results?
  - Can plugins call host APIs (clipboard, filesystem, HTTP)?
  - Capability-based permissions?
- **Frontend rendering**:
  - Do plugins provide raw data and the host renders it?
  - Or do plugins provide React components (via some mechanism)?
  - Or do plugins provide HTML fragments?
- **Plugin distribution**: How are plugins installed?
  - Local directory with WASM files?
  - Plugin registry / marketplace?
  - Git repos?
- **Hot reloading**: Can plugins be updated without restarting?
- **Performance**: WASM startup time, memory limits, query timeout

## Design constraints

- Plugins must be sandboxed — no arbitrary filesystem/network access
  without explicit permission
- The plugin API should be stable across torchsnap versions
- Plugin results must integrate seamlessly into the unified result
  list alongside built-in results
- Latency matters — plugin search should complete within ~100ms
  for responsive UX
