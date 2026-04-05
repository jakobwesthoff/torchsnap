# 23. Call all query plugins regardless of prefix registration

Date: 2026-03-29

## Status

Accepted

Amends [12. Use prefix-based exclusive routing for query plugins](0012-use-prefix-based-exclusive-routing-for-query-plugins.md)

## Context

ADR 0012 established that query plugins with registered prefixes are
**only** called when their prefix matches. In the no-prefix search path,
the host skips any plugin whose `prefixes()` returns a non-empty slice:

```rust
if !plugin.is_enabled() || !plugin.prefixes().is_empty() {
    continue;
}
```

This creates a hard coupling: a plugin that registers a prefix for
exclusive routing cannot also participate in the general (no-prefix)
search pass. The calculator plugin needs both:

1. **Prefix mode** (`=`): exclusive routing, full custom UI with history.
2. **Heuristic mode** (no prefix): detect math expressions in general
   queries, show inline results above the standard result list.

Under ADR 0012, this is impossible with a single plugin instance. The
plugin must either register `=` as a prefix (and be excluded from
general queries) or register no prefix (and lose exclusive routing).

Alternatives considered:

- **Two plugin instances**: Register a prefix-mode and a heuristic-mode
  plugin separately, sharing state. Works but is architecturally awkward
  — two IDs, two settings namespaces, two registry entries for what is
  logically one plugin.
- **Opt-in trait method** (`search_without_prefix() -> bool`): Only
  call prefix-having plugins in the general pass when they opt in.
  Adds a method to the trait for a distinction that can be handled
  implicitly by the plugin returning `Nothing`.
- **Remove the prefix exclusion**: Call all plugins on every query.
  Plugins that have nothing to contribute return `Nothing` (ADR 0021).
  Simple, no new trait surface.

## Decision

Remove the prefix-based exclusion from the no-prefix search path. All
enabled query plugins are called with `search(query, None)` when no
prefix matches, regardless of whether they have registered prefixes.

```rust
// Before (ADR 0012):
if !plugin.is_enabled() || !plugin.prefixes().is_empty() { continue; }

// After:
if !plugin.is_enabled() { continue; }
```

> **Note (ADR 0025):** The `plugin.is_enabled()` trait method shown in both
> examples was later replaced by host-managed enable gating. The host now
> checks `slot.enabled` on its `PluginSlot` wrapper rather than delegating to
> a trait method. The structural change described in this ADR — removing the
> `!plugin.prefixes().is_empty()` guard — is unaffected by that replacement.

### Plugin responsibility

Plugins that register prefixes and do not want to participate in general
queries must return `SearchResponse::Nothing` when `matched_prefix` is
`None`. This is a one-line early return.

### Safety

As specified in ADR 0021, `SearchResponse::CustomUI` returned without a
prefix match is downgraded to `SearchResponse::Results` by the host.
This prevents a plugin from accidentally taking over the full UI during
a general query.

### Existing plugin impact

The only currently affected plugin is the emoji picker, which registers
the `:` prefix. It must be updated to return `Nothing` when
`matched_prefix` is `None`. This is a trivial change.

### Prefix routing is unchanged

Exclusive prefix routing (ADR 0012) is not affected. When the query
matches a registered prefix, only the owning plugin is called — catalog
plugins and other query plugins are still skipped entirely. The change
only affects the no-prefix fallback path.

## Consequences

- A single plugin instance can participate in both prefix-exclusive and
  general search, eliminating the need for awkward multi-instance
  workarounds.
- All query plugins see all non-prefix queries. For most plugins this
  is a fast `Nothing` return, but it does add per-plugin function call
  overhead to every search. This is negligible for the current plugin
  count but should be monitored if the number of query plugins grows
  significantly.
- Plugins with prefixes must now handle the `matched_prefix: None` case
  explicitly. Forgetting to do so could produce unexpected results in
  general queries. The `Nothing` variant (ADR 0021) makes this easy and
  its intent obvious.
- The `prefixes()` method retains its original purpose: declaring which
  prefixes trigger exclusive routing. It no longer implicitly controls
  whether the plugin participates in general search.
