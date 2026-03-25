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
- **Dynamic keybindings**: Plugins need to register keybindings
  for their actions at runtime. A clipboard manager might bind
  Cmd+1..9 to paste recent entries when its results are shown.
  Keybindings should activate/deactivate based on plugin state
  (selected result, active view). The keybinding engine already
  supports layered priority and `active` flags — plugins need a
  way to hook into that system, likely via the plugin ↔ host API.
- **Dependencies**: Can plugins depend on shared libraries or
  other plugins?
- **Versioning**: How do plugins declare compatibility with
  torchsnap versions? Semver contract on the plugin API?

## Approach: Build internal plugins first, extract API later

Rather than designing the plugin API upfront, build 2–3 plugins
internally first (app launcher, calculator, system commands are
good candidates). Use the patterns that emerge to inform the real
API surface. Extract the plugin interface once we have concrete
evidence of what plugins actually need from the host.

## Plugin settings / config pages

Plugins need their own configuration UI. Design questions:

- **Settings UI model**: macOS-style settings with a sidebar
  listing sections (General, Appearance, Shortcuts, then one
  entry per plugin). Each plugin gets its own card/page.
- **How plugins provide settings UI**: Do plugins declare a
  settings schema (JSON schema → auto-generated form)? Or do
  they provide a React component that gets mounted? Schema is
  safer and more consistent; custom components are more flexible.
- **Settings storage**: Plugin settings in the same
  `settings.json` store, namespaced by plugin ID? Or separate
  store per plugin?
- **Settings access**: How does the plugin read its own config?
  Passed at init? Query on demand? Reactive (notified on change)?

## Options to consider

1. **WASM-first** (Extism, wasmtime): Maximum sandboxing, language
   agnostic, but limited host API access and higher latency.
2. **Native Rust plugins** (dylib): Maximum performance, full API
   access, but no sandboxing and Rust-only.
3. **Scripting layer** (Lua/Deno): Easy authoring, decent
   sandboxing, but another runtime dependency.
4. **Hybrid**: Built-in plugins as native Rust, third-party plugins
   as WASM. Different trust levels.
