# Switch catalog matching from Matcher to Nucleo<T> async worker

The initial search implementation uses nucleo's synchronous `Matcher`
API — iterate all entries, score each one per keystroke. This is fine
for small catalogs (built-in commands) but won't scale to hundreds
or thousands of entries (app launcher, contacts, emoji).

## When to switch

Once a catalog gadget exceeds ~100 entries or when keystroke-to-result
latency becomes noticeable.

## What changes

- Replace per-search `Matcher` iteration with a long-lived `Nucleo<T>`
  instance per catalog
- Entries are injected once (and on catalog updates), not re-scanned
  per query
- Pattern updates happen on the Nucleo worker thread, results arrive
  via a notify callback
- Bridge the notify callback to the Tauri channel-based search
  command (condvar, tokio channel, or similar)

## Open questions

- Whether each catalog gadget gets its own `Nucleo<T>` instance or
  all catalogs share one (separate is cleaner for isolation, shared
  is simpler for cross-catalog ranking)
- Whether to expose Nucleo instances to WASM gadgets or keep them
  host-internal (leaning toward host-internal — gadgets just provide
  data, host owns matching)
- Thread pool sizing for multiple concurrent Nucleo workers
