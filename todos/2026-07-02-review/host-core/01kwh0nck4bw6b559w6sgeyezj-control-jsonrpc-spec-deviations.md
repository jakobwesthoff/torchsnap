# Control API deviates from JSON-RPC 2.0 in three documented-adjacent ways

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/control/mod.rs

## Problem

`docs/control-api.md:50-51` says the Control API "uses JSON-RPC
2.0" and links the spec. `process_request`
(`src-tauri/src/control/mod.rs:215-251`) diverges from the spec
in three ways:

1. **`jsonrpc` member never validated.** The envelope check only
   looks for `method` (`mod.rs:224-227`). A request without
   `"jsonrpc": "2.0"` (or with any other value) is accepted. The
   spec requires the member and requires rejecting requests
   missing it as invalid.
2. **Notifications get responses.** A request without `id` is a
   notification per spec ("The Server MUST NOT reply to a
   Notification"). Here a missing `id` defaults to `Value::Null`
   (`mod.rs:223`) and a full response with `"id": null` is
   written back. A spec-conforming client that fires
   notifications will read unexpected response lines and can
   misassociate them with later requests.
3. **Batch requests are rejected as invalid.** A JSON array of
   requests hits `parsed.get("method")` on a non-object, which
   returns `None`, producing a single `-32600` error
   (`mod.rs:224-227`) instead of a batch response array.

None of these matter for the shipped `socat` examples, but the
doc's unqualified "JSON-RPC 2.0" claim invites clients built on
generic JSON-RPC libraries, which commonly use notifications and
batches.

## Impact

Off-the-shelf JSON-RPC 2.0 client libraries can mis-parse the
stream (unexpected `id: null` responses) or fail on batch sends.
No impact for the documented shell-scripting usage.

## Suggested fix

Either implement the three behaviors (validate `jsonrpc`, skip
responses for id-less requests, iterate arrays) or scope the doc
claim to "a JSON-RPC 2.0 subset: single requests with ids;
notifications and batches unsupported". The doc-only fix is
cheap and honest; pick per intended audience.
