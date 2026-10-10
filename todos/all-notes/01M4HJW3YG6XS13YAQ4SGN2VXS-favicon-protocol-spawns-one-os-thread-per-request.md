---
title: "Favicon protocol spawns one OS thread per request"
kind: improvement
component: backend
impact: low
---
# Favicon protocol spawns one OS thread per request

The `torchsnap-favicon://` protocol handler spawns a fresh OS thread for every request, so a request burst creates an unbounded number of short-lived threads. The work is blocking, so offloading is right, but a raw spawn per request has no limit.

## Problem
`register_favicon_protocol` (`src-tauri/src/network/website_metadata/protocol.rs:88-97`) calls `std::thread::spawn` per request to run the blocking
`FaviconStore::resolve` and `std::fs::read`. A request burst therefore creates an unbounded number of short-lived OS threads.

## Proposal
Replace `std::thread::spawn` with `tauri::async_runtime::spawn_blocking`
(a bounded pool). The gadget asset protocol has the same pattern and the
same fix, tracked in the linked todo.

## Relations

- Relates to: [Gadget asset protocol spawns one OS thread per request](01KWFZ4KKAQ7SPWNM2NCKET1FT-gadget-asset-protocol-spawns-one-os-thread-per-request.md), the same thread-per-request fix, to mirror
