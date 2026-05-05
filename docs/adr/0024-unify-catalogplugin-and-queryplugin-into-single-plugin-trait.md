# 24. Unify CatalogPlugin and QueryPlugin into a single Plugin trait

Date: 2026-04-05

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

Amends [12. Use prefix-based exclusive routing for query plugins](0012-use-prefix-based-exclusive-routing-for-query-plugins.md)

## Context

The system previously had two separate traits: `CatalogPlugin` (for plugins
with finite entry lists scored by nucleo) and `QueryPlugin` (for plugins that
run their own matching). Both traits shared ten identical lifecycle methods —
`id()`, `is_enabled()`, `enabled_settings_key()`, `initialize_settings()`,
`setup()`, `teardown()`, `execute()`, `shortcuts()`, `handle_shortcut()`, and
`handle_message()` — duplicated with no common supertrait. A plugin that
needed both catalog entries and custom query results (like the bangs plugin
providing static entries but also dynamic URL resolution) had to choose one
trait, making the dual-mode use case impossible without two plugin instances.

## Decision

Replace both `CatalogPlugin` and `QueryPlugin` with a single `Plugin` trait.
Catalog-only plugins override `entries()`. Query-only plugins override
`search()` (and optionally `search_prefixes()`). Hybrid plugins override both.
All lifecycle methods appear once. The host checks whether `entries()` and/or
`search()` return non-default values to route search queries.

````rust
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit { settings }
    fn enable(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {}
    fn disable(&self) {}
    fn setting_changed(&self, _key: &str, _value: serde_json::Value) {}
    fn execute(&self, entry_id: &str, action_id: &ActionId, app: &tauri::AppHandle) -> anyhow::Result<PostAction>;
    fn shortcuts(&self) -> Vec<PluginShortcut> { vec![] }
    fn handle_shortcut(&self, _shortcut_id: &str, _app: &tauri::AppHandle) -> anyhow::Result<PostAction> { Ok(PostAction::Nothing) }
    fn handle_message(&self, _method: &str, _payload: serde_json::Value, _channel: tauri::ipc::Channel<serde_json::Value>) -> anyhow::Result<serde_json::Value> { anyhow::bail!(...) }
    fn search_prefixes(&self) -> &[String] { &[] }
    fn entries(&self) -> Vec<CatalogEntry> { vec![] }
    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<PluginResponse> { None }
}
````

Registration is a single `host.register()` call. The old `register_query()`
method and the `CatalogPlugin`/`QueryPlugin` trait names are removed.

## Consequences

* No trait duplication — lifecycle methods exist once.
* Plugins can now serve both catalog entries and custom query results with a
  single implementation and a single plugin ID.
* Registration is simpler: one call instead of two conditional calls depending
  on which trait the plugin implements.
* The `register_query()` method is removed. Any code that distinguished
  between catalog and query registration at the call site must be updated.