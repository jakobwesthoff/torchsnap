# Retention cleanup can delete favicons that are still referenced

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/network/website_metadata/cache.rs, favicon_store.rs, mod.rs

## Problem
The retention thread periodically runs
(`src-tauri/src/network/website_metadata/mod.rs:323-343`):

```rust
let valid_keys = cache::evict_expired(&service.db, ttl_days);
service.favicons.cleanup(&valid_keys);
```

`FaviconStore::cleanup` deletes every file whose key is *not* in
`valid_keys` (`favicon_store.rs:110-119`). Two ways live
favicons end up deleted:

1. **A failed SELECT reads as "no favicons are valid".**
   `evict_expired` returns the remaining keys via
   `query_map(...).unwrap_or_default()`
   (`cache.rs:122-127`) — any SQL error (locked db, transient
   I/O) yields an *empty* vec, indistinguishable from "no rows".
   `cleanup(&[])` then deletes **every** favicon file on disk
   while all the metadata rows (still within TTL) keep their
   `favicon_key` references. Until those rows expire — up to
   `cacheTtlDays`, default 30 days — every affected domain
   serves a `torchsnap-favicon://` URL that 404s.

   The whole module follows this swallow-errors style
   (`store` and the DELETE in `evict_expired` are `let _ =`,
   `lookup` is `.ok()` — `cache.rs:94-127`), which is defensible
   for a cache *except* in this one spot where an error output
   feeds a destructive operation.

2. **Race with an in-flight fetch.** `fetch_and_cache_inner`
   stores the favicon file first and writes the DB row after
   (`mod.rs:686-711`). A retention pass between the two steps
   sees an unreferenced file, deletes it, and the row is then
   written pointing at a file that no longer exists — same
   dangling-URL symptom, narrower window.

## Impact
Sporadic mass-loss of cached favicons (icons degrade to the
`globe-alt` fallback), persisting for up to the full TTL because
the metadata rows still count as fresh cache hits and no
re-fetch is triggered. Hard to reproduce; looks like random icon
breakage.

## Suggested fix
Make `evict_expired` return `Result` (or `Option`) so the caller
can skip `favicons.cleanup` when the key set could not be
determined — never treat a query failure as an empty set when
the consumer deletes based on absence. For the race, either
grace-period new files (skip files younger than a few minutes in
`cleanup`) or hold a short-lived "in flight" key set the
retention pass respects. A log line on swallowed cache errors
would make both failure modes diagnosable.
