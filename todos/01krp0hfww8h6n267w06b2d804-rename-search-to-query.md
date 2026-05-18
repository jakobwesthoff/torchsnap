# Rename `search` interface to `query`

The WIT `search` interface combines catalog entries, query-driven search, and
execution. The `search` function within it is specifically the query-mode entry
point, but its name collides with the interface name, making the distinction
between the two search modes unclear in documentation and code.

Rename `search::search(...)` to `search::query(...)` (or rename the interface
itself to `query`). Cascade the rename through the type names as well:
`SearchResponse` → `QueryResponse`, `SearchGuest` → `QueryGuest` (or similar).

Affects: WIT definition, `torchsnap-gadget-sdk` generated bindings and prelude
re-exports, all gadget crates, `WasmGadgetBridge`, host `Gadget` trait.
