# Inline UI priority ordering for competing gadgets

## Problem

When multiple query gadgets return `SearchResponse::InlineUI` for the same
query (no-prefix path), only one inline slot is available. Currently resolved
by first-registered-wins, which is arbitrary and not user-controllable.

## Proposed solution

Add a configurable priority order in settings for all gadgets that provide
inline UI. The host consults this order when multiple gadgets claim inline for
the same query, and the highest-priority gadget wins.

This is similar to how prefix conflict resolution is deferred (see
`query-prefix-conflict-resolution` todo) — both need a general gadget priority
system that the user can configure.

## Context

Decided during calculator gadget planning. Currently only the calculator gadget
uses `InlineUI`, so first-wins is sufficient. This becomes relevant once a
second inline-capable gadget is added.
