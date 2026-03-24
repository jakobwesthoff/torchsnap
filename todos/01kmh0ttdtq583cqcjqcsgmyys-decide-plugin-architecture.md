# Decide: Plugin architecture and execution model

Open architectural decision about how plugins are structured,
discovered, loaded, and executed.

## Questions to resolve

- **Plugin format**: WASM modules? Rust crates compiled to dylib?
  Scripting language (Lua, JS)? Multiple formats?
- **Execution model**: In-process (WASM sandbox) vs out-of-process
  (child process with IPC)? Tradeoffs between latency and isolation.
- **Plugin lifecycle**: Loaded at startup or on-demand? Can plugins
  run background tasks (clipboard monitor, file indexer) or only
  respond to queries?
- **Plugin manifest**: What metadata does a plugin declare?
  Name, version, trigger prefix, capabilities needed, settings
  schema?
- **Query routing**: How does the host decide which plugins receive
  a query? All plugins see every query? Prefix-based routing
  (`= ` → calculator)? Priority/ordering?
- **Plugin ↔ host API surface**: What can a plugin call?
  Clipboard, filesystem, HTTP, notifications, system commands?
  Capability-based permissions?
- **State**: Can plugins persist state between queries? Between
  app restarts? Where does plugin state live?
- **Dependencies**: Can plugins depend on shared libraries or
  other plugins?
- **Versioning**: How do plugins declare compatibility with
  torchsnap versions? Semver contract on the plugin API?

## Options to consider

1. **WASM-first** (Extism, wasmtime): Maximum sandboxing, language
   agnostic, but limited host API access and higher latency.
2. **Native Rust plugins** (dylib): Maximum performance, full API
   access, but no sandboxing and Rust-only.
3. **Scripting layer** (Lua/Deno): Easy authoring, decent
   sandboxing, but another runtime dependency.
4. **Hybrid**: Built-in plugins as native Rust, third-party plugins
   as WASM. Different trust levels.
