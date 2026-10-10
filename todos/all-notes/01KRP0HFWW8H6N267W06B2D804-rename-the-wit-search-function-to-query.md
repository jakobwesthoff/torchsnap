---
title: "Rename the WIT `search` function to `query`"
kind: improvement
component: gadget-sdk
tags: [api-design]
---
# Rename the WIT `search` function to `query`

The WIT `search` interface combines catalog entries, query-driven search and execution. Its `search` function is the query-mode entry point, but the function name collides with the interface name. That blurs the distinction between the two search modes in documentation and code.

## Proposal

Rename `search::search(...)` to `search::query(...)`, or rename the interface itself to `query`. Cascade the rename through the type names as well: `SearchResponse` becomes `QueryResponse`, and `SearchGuest` becomes `QueryGuest` (or similar).

## Affected code

WIT definition, `torchsnap-gadget-sdk` generated bindings and prelude re-exports, all gadget crates, `WasmGadgetBridge`, and the host `Gadget` trait.
