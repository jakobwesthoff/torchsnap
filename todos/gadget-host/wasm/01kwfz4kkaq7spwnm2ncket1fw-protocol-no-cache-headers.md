---
kind: improvement
severity: low
status: open
area: [src-tauri/src/wasm/protocol.rs]
---

# Gadget asset responses carry no caching headers

## Problem
Successful asset responses set only `Content-Type` and
`Access-Control-Allow-Origin`
(`src-tauri/src/wasm/protocol.rs:90-95`). There is no
`Cache-Control`, `ETag`, or `Last-Modified`. Every asset request
from the webview therefore re-enters the protocol handler, which
re-reads the file from the gadget source; for `.torchsnap`
archives that means zip decompression per request, serialized
behind the `ArchiveSource` Mutex (per the comment at `:60-61`).

## Impact
Repeated launcher opens re-fetch identical JS/CSS bundles.
Wasted CPU on zip decompression and avoidable latency on
custom-UI gadgets with many assets. No correctness impact.

## Suggested fix
Gadget sources are immutable while loaded (archives are never
rewritten in place; directory sources change only during
development). A `Cache-Control: max-age=...` for archive-backed
sources, with `no-cache` for directory sources to keep the dev
loop honest, would eliminate most repeat reads. An alternative
is an in-memory LRU of decompressed assets keyed by
`(gadget_id, path)`. Decide whether webview caching of the
custom scheme actually works on all platforms (verify on macOS
WKWebView first) before building the LRU.
