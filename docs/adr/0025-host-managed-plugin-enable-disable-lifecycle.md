# 25. Host-managed plugin enable/disable lifecycle

Date: 2026-04-05

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

Supersedes [14. Pass app handle to plugin setup](0014-pass-app-handle-to-plugin-setup.md)

Supersedes [15. Add teardown to plugin lifecycle](0015-add-teardown-to-plugin-lifecycle.md)

## Context

Previously, each plugin managed its own enable/disable state independently:
each implemented `is_enabled()` backed by `Arc<AtomicBool>`,
`enabled_settings_key()` returning `Some("enabled")`, and spawned its own
`SettingsWatch<bool>` thread in `setup()` to toggle the flag. This resulted in
approximately 30 lines of duplicated boilerplate per plugin and inconsistent
behavior — some plugins used `ctx.notifier.watch("enabled")`, others used
different patterns.

Additionally, `setup()` and `teardown()` were one-shot: a plugin that was
disabled could not be re-enabled without restarting the application.

## Decision

1. Replace `setup()`/`teardown()` with `enable()`/`disable()` that may be
   called multiple times throughout the plugin's lifetime.
2. Replace `is_enabled()`/`enabled_settings_key()` with a host-managed
   `AtomicBool` per plugin.
3. The host stores an `enabled.<plugin-id>` key at the top level of the
   settings store, not under `plugins.<id>.*`. Plugins cannot access or modify
   their own enabled state.
4. Add `setting_changed(key, value)` so plugins receive notifications about
   their own settings changes without needing `SettingsWatch` threads.
5. The host wraps each plugin in a `PluginSlot`:

```rust
struct PluginSlot {
    plugin: Arc<dyn Plugin>,
    enabled: AtomicBool,
    dispatcher: CoalescingDispatcher,
}
```

6. `PluginContext` no longer carries a `notifier` field — only `settings` and
   `frecency`.
7. On first startup after upgrade, existing `plugins.<id>.enabled` values are
   migrated to `enabled.<id>`.

## Consequences

- Zero boilerplate for enable/disable in plugins — just implement `enable()`
  and `disable()`. The ~30 lines of `AtomicBool` + `SettingsWatch` setup per
  plugin are gone.
- Plugins can be toggled on/off at runtime without an app restart.
- The host is the single authority on whether a plugin is active — no
  inconsistency possible between the stored setting and the plugin's internal
  flag.
- `PluginSettingsNotifier` is no longer part of the plugin API surface (it
  remains in use internally by `FrecencyStore`, the control socket, etc.).
- Old methods removed: `setup()`, `teardown()`, `is_enabled()`,
  `enabled_settings_key()`.
