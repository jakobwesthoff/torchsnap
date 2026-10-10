---
title: "Replace Origin reflection on the favicon protocol with an allowlist"
kind: improvement
component: backend
impact: low
tags: [security]
---
# Replace Origin reflection on the favicon protocol with an allowlist

The `torchsnap-favicon://` protocol copies the request `Origin` into `Access-Control-Allow-Origin` and falls back to `*`. This is a consistency hardening item, not a leak: the scheme serves favicon image bytes, which are not secrets.

## Problem
`handle_request` (`src-tauri/src/network/website_metadata/protocol.rs:109-131`) reflects the request `Origin`
into `Access-Control-Allow-Origin`, falling back to `*`. There is no
`Access-Control-Allow-Credentials`, so reflection is equivalent to
`*`. The scheme serves favicon image bytes to the host webview via
`<img>`. A request with no Origin gets the `*` fallback, which is normal traffic. Favicon bytes
are not secrets, so the incremental read risk is negligible.

## Proposal
The gadget asset scheme has the same pattern and an origin allowlist is
proposed for it in the linked todo. Apply the identical fix here for
consistency, not because this scheme leaks anything sensitive. Replace
the Origin echo with the same origin allowlist, and factor the allowlist
so both schemes share it.

## Relations

- Relates to: [Replace Origin reflection on the gadget asset protocol with an allowlist](01KWH4J9BPTRAYF451YZD2145B-replace-origin-reflection-on-the-gadget-asset-protocol-with-an-allowlist.md), the same origin allowlist fix, to mirror
