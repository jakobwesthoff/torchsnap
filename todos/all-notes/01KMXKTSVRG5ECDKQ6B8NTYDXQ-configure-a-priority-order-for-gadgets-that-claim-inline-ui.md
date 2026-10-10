---
title: "Configure a priority order for gadgets that claim inline UI"
kind: feature
component: gadget-host
origin: discussion
---
# Configure a priority order for gadgets that claim inline UI

Several query gadgets can claim the single inline slot, and the host resolves a conflict by first-registered-wins. That rule is arbitrary and not under the user's control. A configurable priority order fixes this, and it should be designed together with the deferred prefix conflict resolution, which needs the same general gadget priority system.

## Problem

When multiple query gadgets return `SearchResponse::InlineUI` for the same query (no-prefix path), only one inline slot is available. Currently resolved by first-registered-wins, which is arbitrary and not user-controllable.

## Proposal

Add a configurable priority order in settings for all gadgets that provide inline UI. The host consults this order when multiple gadgets claim inline for the same query, and the highest-priority gadget wins.

The deferred prefix conflict resolution needs a general gadget priority system that the user can configure, too. Both should share that system.

## Context

Decided during calculator gadget planning. Currently only the calculator gadget uses `InlineUI`, so first-wins is sufficient. This becomes relevant once a second inline-capable gadget is added.
