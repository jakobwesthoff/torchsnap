# Panic in gadget asset handler leaves the request hanging

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/wasm/protocol.rs

## Problem
The protocol handler runs `handle_request` on a detached thread
and only calls `responder.respond(...)` at the end
(`src-tauri/src/wasm/protocol.rs:62-65`). Two panic paths exist
inside `handle_request`:

- `registry.read().expect("registry not poisoned")` (`:140`)
- the `expect("valid response")` builders (`:95`, `:101`)

If any of these fire, the spawned thread unwinds, the `responder`
is dropped without responding, and the webview request never
completes. The `Origin` echo at `:93` also feeds an arbitrary
header value into `.header(...)`; a malformed Origin value is the
one realistic way `expect("valid response")` could trip.

## Impact
A single poisoned lock or malformed header turns into a silently
hanging asset request instead of a 500. Hard to diagnose because
nothing is logged.

## Suggested fix
Wrap the body in `catch_unwind` (or restructure so all fallible
steps return `Result`) and respond with a 500 on failure, logging
the error. Validate the echoed Origin via
`http::HeaderValue::from_str` and fall back to a fixed value when
invalid.
