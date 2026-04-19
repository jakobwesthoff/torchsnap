# Make WIT guest exports optional where the plugin doesn't opt in

## Context

`wit/torchsnap-plugin.wit` declares every guest interface as a
mandatory export on `world plugin`:

```wit
world plugin {
  import logging;
  import settings;
  import sql;
  import clipboard;
  import frecency;

  export lifecycle;
  export search;
  export messaging;
  export tasks;
}
```

wasmtime's linker refuses to instantiate a component whose WIT world
promises exports the guest doesn't provide, so every plugin must ship
an implementation for every export — even when the plugin has no use
for that surface.

Concrete cost: both the `template` and `emoji-picker` plugins carry
stub `MessagingGuest` / `TasksGuest` impls that just return
`Err("…")`. The emoji picker also has an empty `entries()` because
it is prefix-only. This is load-bearing noise — a plugin author must
know WIT semantics well enough to understand why their plugin doesn't
compile unless they add the stubs.

The mismatch is visible because the manifest side *is* already
optional: `[[tasks]]` being absent makes the host skip the scheduler
loop, and no `[frontend.settings]` component means the host never
dispatches into `on-setting-changed`. The guest-side exports should
mirror the manifest — declaring a stub `run_task` that returns an
error for a plugin that declared no tasks in its manifest is
structurally redundant.

## Scope

Every export in `world plugin` that isn't load-bearing for a minimal
plugin. Today that is:

- `messaging` (not every plugin has frontend↔backend RPC)
- `tasks` (not every plugin runs scheduled work)
- `search::entries` (catalog-only surface; prefix-only plugins
  return an empty `Vec`)

`lifecycle` (`enable` / `disable` / `on-setting-changed`) and
`search::search` stay mandatory — every plugin has *some* activation
path and *some* way to respond to input.

## Options to explore

### 1. Sub-worlds per capability

Split `world plugin` into a small core plus capability mixins. A
plugin author then picks the smallest world that fits:

```wit
world plugin-base {
  import logging;
  import settings;
  ...
  export lifecycle;
  export search;
}

world plugin-with-messaging {
  include plugin-base;
  export messaging;
}

world plugin-with-tasks {
  include plugin-base;
  export tasks;
}

world plugin { // default all-in — used by the template
  include plugin-with-messaging;
  include plugin-with-tasks;
}
```

The host would need to try-instantiate against each world in
decreasing-feature order, or read the manifest first to pick which
world to link. The host's bindgen invocation would also need to
generate code for each variant, which likely means selecting the
widest world and using wasmtime's "optional export" behaviour
(see option 3) rather than runtime world selection.

### 2. Host-side feature detection via manifest

Add a `[capabilities]` block (or extend `[plugin]`) in `manifest.toml`
that lists which exports the plugin actually provides, e.g.:

```toml
[plugin]
id = "emoji-picker"
capabilities = ["search"]
```

The loader reads this and only wires up the exports the manifest
declares. Missing capability → host silently declines to call that
export (e.g. `handle_message` returns an error without crossing the
WIT boundary).

Downside: the manifest becomes the source of truth for what the WIT
world says, so a mismatch between "declared capability" and "actual
WIT exports" would surface as a link failure at enable time — not
much safer than the current status quo.

### 3. wasmtime "optional exports"

Check whether wasmtime's component-model linker offers a way to
declare exports as optional — the binding would expose a guard
method (`is_some()` / `has_export()`) and a no-op default for
plugins that don't implement it. If this is supported, it's the
least-invasive option: the WIT world stays the same and the host
just gates its dispatch on the guard before invoking the guest.

Starting point: the wasmtime `bindgen!` macro's option set (see
`src-tauri/src/wasm/bindings.rs`) and the component-model spec
around resource/interface optionality.

## Decision criteria

- Plugin author ergonomics — adding a new plugin shouldn't require
  boilerplate rejections for surfaces the plugin doesn't use.
- Host simplicity — any of these options that needs runtime
  feature probing has to be weighed against the current "always
  call; guest returns Err" pattern, which is ugly but trivial.
- Compatibility with the current `plugin` world — a new world
  name is fine for new plugins but we also want existing
  `template` / `calculator` / `emoji-picker` to migrate cleanly.

## Related

- `plugins/template/src/lib.rs` — ships full stubs as a teaching
  example; any refactor should update the template to
  demonstrate the new opt-in pattern.
- `plugins/emoji-picker/src/lib.rs` — a good real-world case:
  prefix-only, no messaging, no tasks. The `entries()`/
  `handle_message`/`run_task` impls are pure boilerplate after
  the conversion.
- `todos/wasm/01kpk8jp3s737phqs239s9vt59-plugin-assets-wit-interface.md`
  — another WIT surface that would benefit from an opt-in
  capability flag so the binding isn't generated for plugins
  that don't ask for it.
