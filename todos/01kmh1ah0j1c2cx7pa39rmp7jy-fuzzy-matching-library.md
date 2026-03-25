# Fuzzy matching library

## Decisions made

See [ADR 0011](../docs/adr/0011-use-tauri-channels-for-streaming-search-results-from-catalog-and-query-plugins.md)
for the full architectural decision.

**Library**: nucleo (from helix editor). Best-in-class Rust fuzzy
matcher. UTF-8 aware, very fast, supports match highlighting.

**Where it runs**: Rust side only. The frontend never performs fuzzy
matching — it receives pre-filtered, pre-scored results with match
highlight positions.

**What it covers**: Catalog plugin entries only. Query plugins
handle their own filtering and return pre-scored results.

## Implementation plan

1. Add `nucleo` crate dependency
2. Build a catalog registry in Rust that holds entry lists from
   catalog plugins
3. On `search(query, channel)`, run nucleo against all registered
   catalogs, send `CatalogResults` through the channel
4. Match highlight positions are included in `ScoredEntry` so the
   frontend can render highlighted characters

## Plugin configurability

Catalog plugins can configure their nucleo matching:

- **Match targets**: Which fields are matchable (title, subtitle,
  keywords) and their relative weight
- **Minimum score threshold**: Stricter or looser matching per
  plugin

Query plugins bypass nucleo entirely — they own their own matching.

## Still open

- Exact nucleo API integration (Matcher vs Nucleo struct)
- Background thread for large catalogs or inline for small ones
- Whether to expose nucleo functions to WASM plugins for their
  internal use (deferred — query plugins handle their own matching)
