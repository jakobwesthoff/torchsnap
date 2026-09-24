---
kind: question
status: open
---

# Add `data` field to `catalog-entry`?

`scored-entry` has an opaque `data: option<string>` field that round-trips
through `execute()`. `catalog-entry` does not. When the host converts a catalog
entry to a scored entry during fuzzy matching, there is no way for the gadget
to attach context that `execute()` can read back without matching on `entry.id`.

Needs discussion: is this actually useful? Catalog entries already have a
stable `id` that `execute()` can dispatch on, and the data set is static. The
`data` field matters more for query-mode results where entries are generated
dynamically. Adding it to `catalog-entry` would be a WIT change affecting the
SDK bindings and all gadgets.
