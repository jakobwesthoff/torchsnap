# Rename/restructure the `search` module on the Rust side

## Problem

The `src-tauri/src/search/` module is named "search", but the entire launcher
is fundamentally about searching. The name doesn't distinguish this module's
responsibility from the application's core purpose.

The module currently contains:

- `mod.rs` — Tauri commands (`search`, `execute_action`) that bridge frontend
  requests to the catalog registry
- `catalog.rs` — `CatalogRegistry` which aggregates plugin catalogs and
  performs matching
- `types.rs` — shared types (`SearchMessage`, `ActionId`, `PostAction`,
  catalog entry types)

## What to decide

- **Better name**: something like `commands`, `bridge`, `ipc`, or `entrypoint`
  that reflects the module's role as the Tauri command layer rather than
  implying it owns "search" as a concept.
- **Content placement**: consider whether `CatalogRegistry` and the shared
  types belong here at all, or should live closer to the plugin system (e.g.,
  under `plugins/` or a dedicated `catalog/` module), with only the thin Tauri
  command wrappers remaining in this module.
