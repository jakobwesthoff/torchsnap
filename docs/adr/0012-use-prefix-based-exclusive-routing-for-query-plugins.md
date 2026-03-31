# 12. Use prefix-based exclusive routing for query plugins

Date: 2026-03-25

## Status

Accepted

## Context

ADR 0011 established two plugin modes — catalog (host-filtered) and
query (self-filtered) — but did not specify how the search system
routes queries to query plugins. ADR 0008 introduced prefix-triggered
plugin views but did not define the routing mechanism.

Query plugins need a way to declare interest in specific input
patterns. An emoji picker activates on `:`, a calculator on `=`, a
web search on `g `. Without routing, every plugin receives every
query and must check the prefix itself, wasting cycles and creating
ambiguous ownership — who "owns" a prefixed query?

Additionally, some query plugins (e.g., web search fallback) should
run on every query without any prefix, contributing results alongside
catalog plugins.

Alternatives considered:

- **No routing (all plugins see all queries)**: Simple but plugins
  waste cycles on irrelevant queries. No clear ownership means
  catalog plugins could return spurious matches on prefixed input
  (e.g., app launcher matching `:rocket` against app names).
- **Frontend routing**: Frontend strips prefix and sends to a
  specific plugin. Moves routing logic out of Rust, splits
  responsibility, complicates the single `search` command.
- **Plugin-side prefix checking**: Each plugin checks the prefix
  itself and returns empty when not relevant. Works but duplicates
  logic and still routes all queries to all plugins.

## Decision

### Two plugin traits

The plugin system uses two separate traits reflecting the two
fundamentally different plugin modes from ADR 0011:

- **`CatalogPlugin`**: provides a static entry list, host-filtered
  via nucleo. Never sees the query.
- **`QueryPlugin`**: receives the query string, performs its own
  matching, returns pre-scored results.

### Optional prefix registration

`QueryPlugin` declares zero or more prefixes via
`fn prefixes(&self) -> &[&str]`. Prefixes can be multi-character
(e.g., `":"`, `"g "`, `"="`, `"http://"`).

- **Prefix plugins** (`prefixes()` returns non-empty): activated
  only when the query starts with one of their prefixes.
- **Always-on plugins** (`prefixes()` returns empty): receive every
  query, run alongside catalog plugins.

### Exclusive routing

When the query matches a registered prefix:

1. Only the owning query plugin is called. Catalog plugins and
   prefix-less query plugins are **skipped entirely**.
2. The prefix is stripped from the query before passing it to the
   plugin.
3. The matched prefix is passed as `matched_prefix: Option<&str>`
   so the plugin knows which of its prefixes triggered (relevant
   when a plugin registers multiple prefixes).

When no prefix matches:

1. All catalog plugins are searched (host runs nucleo).
2. All always-on query plugins are called.
3. Results are merged and sorted by score.

### Conflict resolution

- **Longest prefix wins**: if plugin A registers `":"` and plugin B
  registers `":e"`, a query `":emoji"` routes to B.
- **First-registered wins**: if two plugins register the same prefix,
  the first one registered takes ownership. A warning is logged.
- Full conflict resolution (user-configurable priority, runtime
  disambiguation) is deferred — see the `query-prefix-conflict-
  resolution` todo.

### Return type

`QueryPlugin::search()` returns a `SearchResponse` whose result vectors
carry `ScoredEntry` values — the plugin-facing type without a `source`
field. The registry wraps each entry in a `SourcedEntry` via
`SourcedEntry::new(plugin.id(), scored_entry)` before forwarding to the
host. This prevents plugins from spoofing another plugin's source.

## Consequences

- Prefix-activated plugins get clean, stripped queries without
  boilerplate prefix checking.
- Catalog plugins never see prefixed input, eliminating spurious
  matches (no app results for `:rocket`).
- Exclusive routing means a prefixed query has exactly one owner —
  no ambiguity, no wasted work.
- Always-on query plugins (no prefix) coexist with catalog plugins
  naturally, contributing results to every search.
- The `matched_prefix` parameter supports plugins with multiple
  prefixes (e.g., a web search plugin handling both `g ` and
  `ddg `).
- Prefix conflict resolution is intentionally minimal (first-
  registered wins). This will need revisiting when third-party
  plugins can register arbitrary prefixes.
- The two-trait split means the registry maintains two plugin
  collections and the setup/execute routing must check both. This
  adds some complexity but keeps each trait focused.
