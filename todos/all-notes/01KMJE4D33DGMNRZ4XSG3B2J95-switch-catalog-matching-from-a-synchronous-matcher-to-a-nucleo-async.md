---
title: "Switch catalog matching from a synchronous Matcher to a Nucleo async worker"
kind: refactor
component: backend
status: needs-discussion
---

# Switch catalog matching from a synchronous Matcher to a Nucleo async worker

The initial search implementation uses nucleo's synchronous `Matcher`
API. It iterates over all entries and scores each one on every
keystroke. That is fine for small catalogs such as the built-in
commands, but it will not scale to hundreds or thousands of entries
(app launcher, contacts, emoji).

## When to switch

Once a catalog gadget exceeds about 100 entries, or when
keystroke-to-result latency becomes noticeable.

## Proposal

- Replace per-search `Matcher` iteration with a long-lived `Nucleo<T>`
  instance per catalog.
- Entries are injected once (and on catalog updates), not re-scanned
  per query.
- Pattern updates happen on the Nucleo worker thread, and results arrive
  via a notify callback.
- Bridge the notify callback to the Tauri channel-based search command
  (condvar, tokio channel, or similar).

## Open questions

- Whether each catalog gadget gets its own `Nucleo<T>` instance or all
  catalogs share one. Separate instances are cleaner for isolation, a
  shared one is simpler for cross-catalog ranking.
- Whether to expose Nucleo instances to WASM gadgets or keep them
  host-internal. The leaning is host-internal: gadgets provide data, and
  the host owns matching.
- Thread pool sizing for multiple concurrent Nucleo workers.
