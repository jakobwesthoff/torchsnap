---
kind: improvement
status: open
area: [gadgets/gadget-sdk/src/lib.rs, gadgets/gadget-sdk/src/messaging.rs, gadgets/bangs/src/lib.rs, gadgets/calculator/src/lib.rs, gadgets/template/src/lib.rs, gadgets/zerotier/src/lib.rs]
tags: [api-design, wasm]
---

# Typed messaging requests in the Rust gadget SDK

Implement this together with the entry-action command redesign
(`todos/gadget-host/api/01kr2357mcz36g4gte0c0t1qz5-entry-action-commands-and-slots.md`,
ADR 55), in the same refactor.

## Problem

WASM gadgets implement `MessagingGuest::handle_message(method: String,
payload: String)` by hand: a `match` on the method string and
per-method JSON decoding into ad-hoc payload structs, for example
`SaveHistoryPayload` in `gadgets/calculator/src/lib.rs:268`. Nothing
links a method name to its payload type at compile time. Four gadgets
do this: bangs, calculator, template, zerotier.

## Decision

Add a typed trait to the Rust SDK with a blanket impl into the
generated `MessagingGuest`, mirroring `Search` → `SearchGuest` from the
command redesign:

```rust
pub trait Messaging {
    type Request: DeserializeOwned;
    fn handle(request: Self::Request) -> Result<serde_json::Value, String>;
}

impl<T: Messaging> MessagingGuest for T { /* ... */ }
```

The blanket impl builds `{"method": <method>, "payload": <payload>}`
from the two WIT strings, decodes `Self::Request`, calls `handle`, and
encodes the response. An unknown method or a payload that does not
match becomes an `Err` before gadget code runs.

Each gadget defines its request enum with
`#[serde(tag = "method", content = "payload", rename_all = "snake_case")]`.

Unchanged:

- The WIT `handle-message` signature.
- The frontend `sendMessage(method, payload)` API, used by launcher
  views and settings panels alike. Splitting messaging into launcher
  and settings channels was discussed and rejected: no gadget
  distinguishes the two, and both are the same gadget's own code.
- `impl_noop_messaging!` for gadgets without messages, and a
  hand-written `impl MessagingGuest` stays possible. A type that
  implements both traits is a compile error (E0119), which is intended.
- Native gadgets keep `Gadget::handle_message` with its streaming
  channel. No typed layer there.

Responses stay a `serde_json::Value`, because methods return different
shapes. Typed responses per variant are not worth the complexity.

## Pitfall: empty payloads

Settings panels send `{}` as the payload for methods without arguments
(`sendMessage("clear_history", {})`). A serde spike showed that an
adjacently tagged unit variant rejects that:

- `ClearHistory` with payload `{}`: error "invalid type: map, expected
  unit variant".
- `ClearHistory` with payload `null` or no payload: ok.
- `ClearAll {}` (empty struct variant) with payload `{}`: ok.

Either the SDK docs tell authors to write argument-less methods as
`Variant {}`, or the blanket impl treats a `{}` payload like a missing
one. Decide during implementation.
