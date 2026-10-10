---
title: "Control API deviates from JSON-RPC 2.0 in three ways"
kind: improvement
component: backend
status: needs-discussion
impact: low
tags: [api-design]
---
# Control API deviates from JSON-RPC 2.0 in three ways

`docs/control-api.md` says the Control API "uses JSON-RPC 2.0" and links the spec. `process_request` in `src-tauri/src/control/mod.rs` diverges from the spec in three ways. None of them matter for the shipped `socat` examples, but generic JSON-RPC client libraries commonly use notifications and batches, so they can mis-parse or fail against the server.

## Problem

1. **`jsonrpc` member never validated.** The envelope check only looks for `method`. A request without `"jsonrpc": "2.0"`, or with any other value, is accepted. The spec requires the member and requires rejecting requests that miss it as invalid.
2. **Notifications get responses.** A request without `id` is a notification per spec, and the server must not reply to it. Here a missing `id` defaults to `Value::Null`, and a full response with `"id": null` is written back. A spec-conforming client that fires notifications reads unexpected response lines and can misassociate them with later requests.
3. **Batch requests are rejected as invalid.** A JSON array of requests hits `parsed.get("method")` on a non-object, which returns `None`. That produces a single `-32600` error instead of a batch response array.

## Impact

Off-the-shelf JSON-RPC 2.0 client libraries can mis-parse the stream (unexpected `id: null` responses) or fail on batch sends. No impact for the documented shell-scripting usage.

## Suggested fix

Either implement the three behaviors (validate `jsonrpc`, skip responses for id-less requests, iterate arrays), or scope the doc claim to "a JSON-RPC 2.0 subset: single requests with ids; notifications and batches unsupported". The doc-only fix is cheap and honest. Pick by the intended audience.
