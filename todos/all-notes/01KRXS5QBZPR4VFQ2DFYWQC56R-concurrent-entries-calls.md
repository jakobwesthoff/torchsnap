---
kind: improvement
status: open
tags: [performance, concurrency]
---

# Make entries() calls concurrent or isolated

`entries()` calls for all catalog gadgets currently run sequentially inside a
single `spawn_blocking` task in `gadget_host.rs` (`search_catalogs_static`,
lines 859-933). A single slow `entries()` implementation delays the entire
catalog batch for every gadget.

This is acceptable while all gadgets are bundled and trusted, but becomes a
serious problem once third-party gadgets are allowed. A malicious or poorly
written gadget could stall the entire catalog pipeline on every keystroke.

## Required change

Move `entries()` calls to the same concurrent `JoinSet::spawn_blocking` pattern
used for `search()` calls, so each gadget's catalog entries are dispatched
independently. A slow gadget should only delay its own catalog results, not
block other gadgets.

## Considerations

- The catalog results are currently sent as a single merged batch. Concurrent
  calls would require either waiting for all to finish (defeating the purpose)
  or streaming partial catalog batches as they arrive (matching how query
  results already work).
- A per-gadget timeout on `entries()` would add a safety net regardless of
  concurrency.
- The frontend catalog merge logic may need adjustment if catalog results
  arrive per-gadget rather than as one batch.
