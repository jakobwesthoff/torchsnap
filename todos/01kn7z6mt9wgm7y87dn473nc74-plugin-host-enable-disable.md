# Refactor: Centralize plugin enable/disable in PluginHost

## Problem

Every plugin that supports an enable/disable toggle currently implements its
own boilerplate:

1. An `AtomicBool` (or `Arc<AtomicBool>`) field for the enabled state
2. Reading the initial value from settings in `setup()`
3. Spawning a thread to watch for changes via `ctx.notifier.watch("enabled")`
4. Implementing `is_enabled()` to read the atomic
5. Implementing `enabled_settings_key()` to return `"enabled"`

This is identical across clipboard, calculator, duckduckgo-bangs, and will be
for every future plugin with a toggle.

## Proposed Solution

Move the enable/disable lifecycle into `PluginHost`:

1. If a plugin returns `Some(key)` from `enabled_settings_key()`, the host
   handles everything:
   - Reads the initial value from settings during `initialize_and_start()`
   - Stores the enabled state in an `AtomicBool` owned by the host (per plugin)
   - Spawns the settings watch thread
   - Checks the state before calling `search()`, `entries()`, etc.
2. Plugins no longer need to implement `is_enabled()` or manage their own
   enabled state. The trait method `is_enabled()` can be removed or made
   private to the host.
3. Plugins that need to react to enabled state changes (e.g., clipboard
   starting/stopping its watcher) can still watch the setting themselves, but
   the gating decision is centralized.

## Benefits

- Eliminates 15-20 lines of boilerplate per plugin
- Single source of truth for enabled state
- Consistent behaviour across all plugins
- Easier to add new plugins

## Considerations

- The clipboard plugin does more than just gate — it starts/stops a watcher
  thread based on enabled state. That reactive behaviour would still live in
  the plugin; only the simple gating and `is_enabled()` reporting would move
  to the host.
