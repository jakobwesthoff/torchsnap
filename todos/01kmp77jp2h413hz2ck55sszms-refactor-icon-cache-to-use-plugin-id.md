# Refactor IconCache to use plugin ID for namespacing

Per ADR 0017, the plugin ID is the canonical identifier for all
per-plugin subsystems. The `IconCache` currently receives a flat
directory (`app_cache_dir/icons/`) and has no concept of which plugin
owns which icons. All plugin icons land in the same directory.

## What needs to change

- `IconCache` should namespace its storage by plugin ID, so each
  plugin's icons are stored under `app_cache_dir/<plugin-id>/icons/`
  (or similar).
- The shared `Arc<IconCache>` pattern can remain, but methods need
  a plugin ID parameter, or each plugin gets its own cache instance
  scoped to its directory.
- Update `AppLauncherPlugin` and `SystemPreferencesPlugin` callers.
