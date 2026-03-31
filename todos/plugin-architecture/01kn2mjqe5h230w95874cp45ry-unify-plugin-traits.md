# Unify CatalogPlugin and QueryPlugin into a Single Plugin Trait

## Decision

Merge `CatalogPlugin` and `QueryPlugin` into one `Plugin` trait. No supertrait
split — a single unified trait with optional methods and sensible defaults.

## Rationale

- The two traits share 10 identical method signatures, causing dual-edit risk
  for every lifecycle change.
- `PluginHost` duplicates dispatch across both types in 7 places
  (`initialize_settings`, `collect_watched_keys`, `setup`, `shortcuts`,
  `execute`, `handle_message`, `teardown`).
- The current design cannot cleanly express a hybrid plugin that provides both
  catalog entries and custom search, because `register()` and
  `register_query()` take separate `Box` types — two instances would be needed,
  duplicating state and splitting frecency.

## Target Design

```rust
trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn is_enabled(&self) -> bool { true }
    fn enabled_settings_key(&self) -> Option<&'static str> { None }
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit { settings }
    fn setup(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {}
    fn teardown(&self) {}
    fn execute(&self, entry_id: &str, action_id: &ActionId, app: &tauri::AppHandle) -> anyhow::Result<PostAction>;
    fn shortcuts(&self) -> Vec<PluginShortcut> { vec![] }
    fn handle_shortcut(&self, _shortcut_id: &str, _app: &tauri::AppHandle) -> anyhow::Result<PostAction> { Ok(PostAction::Nothing) }
    fn handle_message(&self, _method: &str, _payload: Value, _channel: Channel<Value>) -> anyhow::Result<Value> { anyhow::bail!("...") }

    /// Prefixes that activate exclusive search routing for this plugin.
    /// Renamed from `prefixes` for clarity on the unified trait.
    fn search_prefixes(&self) -> &[&str] { &[] }

    /// Return catalog entries for host-side nucleo matching.
    fn entries(&self) -> Vec<CatalogEntry> { vec![] }

    /// Plugin-driven search. Default: no-op (catalog-only plugins).
    fn search(&self, _query: &str, _matched_prefix: Option<&str>,
              _results: &ResultChannel, _cancel: &CancellationToken) {}
}
```

- Catalog-only plugins override `entries()`.
- Query-only plugins override `search()` (and optionally `search_prefixes()`).
- Hybrid plugins override both — the host calls both paths unconditionally.

## Host Changes

- Single `Vec<Arc<dyn Plugin>>` replaces `catalog_plugins` + `query_plugins`.
- Single `register()` method.
- All 7 duplicated dispatch loops collapse into one.
- `ShortcutOwner` enum (Catalog/Query) is eliminated.
- Search routing: host calls `entries()` for the catalog phase on all plugins
  (empty default is free), and spawns `search()` tasks for all plugins (no-op
  default returns immediately).
