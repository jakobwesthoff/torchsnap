# 17. Formalize plugin ID as canonical identifier for all subsystems

Date: 2026-03-27

## Status

Accepted

## Context

Every plugin already has an `id()` method on the `Plugin` trait (formerly
split across `CatalogPlugin` and `QueryPlugin` — see ADR 0024). It is used for search result routing and
`execute_action` dispatch. However, several other subsystems that
operate per-plugin use their own ad-hoc identifiers:

- **Icon cache:** A shared `IconCache` receives a single directory path
  (`app_cache_dir/icons/`) assembled in `lib.rs`. Icons from all
  plugins land in the same flat directory. The cache has no concept of
  which plugin owns which icons.
- **Frontend component registry:** Uses string keys like
  `"emoji-picker"` that happen to match the plugin ID, but this
  correspondence is not enforced — it is a coincidence maintained by
  convention.
- **Future subsystems** (per-plugin storage, per-plugin settings,
  per-plugin cache directories) all need a stable, filesystem-safe
  identifier.

There is no single documented contract that says "the plugin ID is the
one identifier used everywhere."

## Decision

The value returned by `id()` is the **canonical identifier** for a
plugin across all subsystems:

- Search result routing and action dispatch (existing)
- Frontend component registry keys (existing, now formalized)
- Per-plugin cache directories (e.g., `app_cache_dir/<plugin-id>/`)
- Per-plugin host-managed state under
  `app_data_dir/plugin-home/<plugin-id>/` (`sql/`, future
  `files/`, `cache/`). See ADR 0035 for the split between
  `plugins/` (code) and `plugin-home/` (state).
- Per-plugin WASM code at `app_data_dir/plugins/<plugin-id>.torchsnap`
  (user-installed) or `<resource_dir>/plugins/<plugin-id>.torchsnap`
  (bundled), per ADR 0035.
- Per-plugin storage files (e.g., `storage.sqlite3` under the
  plugin's `sql/` subdirectory).
- Per-plugin settings namespacing (e.g., `plugins.<plugin-id>.*`)
- Any future subsystem that operates per-plugin

### Format constraints

Plugin IDs must be:

- Lowercase ASCII, kebab-case (e.g., `clipboard-manager`, `emoji-picker`)
- Filesystem-safe (no `/`, `\`, `.`, spaces, or special characters)
- Unique across all registered plugins (enforced at registration time)
- Stable — changing an ID is a breaking change that orphans stored data

## Consequences

- Plugin IDs become a durable contract. Renaming a plugin ID requires
  a data migration.
- `IconCache` needs a refactor to use plugin IDs for namespacing. This
  can be done incrementally — add a plugin ID parameter to the relevant
  methods and update callers.
- Registration should validate ID format (lowercase kebab-case) and
  uniqueness, failing loudly on violations.
- All future per-plugin subsystems use `id()` without inventing their
  own identifier.
