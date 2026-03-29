# Inline UI priority ordering for competing plugins

## Problem

When multiple query plugins return `SearchResponse::InlineUI` for the same
query (no-prefix path), only one inline slot is available. Currently resolved
by first-registered-wins, which is arbitrary and not user-controllable.

## Proposed solution

Add a configurable priority order in settings for all plugins that provide
inline UI. The host consults this order when multiple plugins claim inline for
the same query, and the highest-priority plugin wins.

This is similar to how prefix conflict resolution is deferred (see
`query-prefix-conflict-resolution` todo) — both need a general plugin priority
system that the user can configure.

## Context

Decided during calculator plugin planning. Currently only the calculator plugin
uses `InlineUI`, so first-wins is sufficient. This becomes relevant once a
second inline-capable plugin is added.
