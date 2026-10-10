---
title: "Retention cleanup can delete favicons that are still referenced"
kind: bug
component: backend
---
# Retention cleanup can delete favicons that are still referenced

The website-metadata retention pass can delete favicon files that cached metadata rows still reference. Those domains then serve `torchsnap-favicon://` URLs that return 404 until the rows expire, which takes up to `cacheTtlDays` (default 30 days). Two paths lead there: a failed SELECT, and a race with an in-flight fetch.

## Problem

Read in the code; not confirmed at runtime. The retention thread in `src-tauri/src/network/website_metadata/mod.rs` runs periodically:

```rust
let valid_keys = cache::evict_expired(&service.db, ttl_days);
service.favicons.cleanup(&valid_keys);
```

`FaviconStore::cleanup` deletes every file whose key is not in `valid_keys`. Two ways live favicons end up deleted:

1. **A failed SELECT reads as "no favicons are valid".** `evict_expired` in `cache.rs` returns the remaining keys via `query_map(...).unwrap_or_default()`. Any SQL error (a locked database, transient I/O) yields an empty vec, which cannot be told apart from "no rows". `cleanup(&[])` then deletes every favicon file on disk, while all the metadata rows (still within TTL) keep their `favicon_key` references. Until those rows expire, every affected domain serves a URL that 404s.

   The whole module follows this swallow-errors style. The store and the DELETE in `evict_expired` use `let _ =`, and `lookup` uses `.ok()`. That is defensible for a cache, except in this one spot, where an error output feeds a destructive operation.

2. **Race with an in-flight fetch.** `fetch_and_cache_inner` in `mod.rs` stores the favicon file first and writes the DB row after. A retention pass between the two steps sees an unreferenced file, deletes it, and the row is then written pointing at a file that no longer exists. That gives the same dangling-URL symptom, over a narrower window.

## Impact

Sporadic mass loss of cached favicons. Icons degrade to the `globe-alt` fallback, and this persists for up to the full TTL, because the metadata rows still count as fresh cache hits and no re-fetch is triggered. The failure is hard to reproduce and looks like random icon breakage.

## Suggested fix

Make `evict_expired` return a `Result` (or an `Option`), so the caller can skip `favicons.cleanup` when the key set could not be determined. Never treat a query failure as an empty set when the consumer deletes based on absence.

For the race, either grace-period new files (skip files younger than a few minutes in `cleanup`), or hold a short-lived "in flight" key set that the retention pass respects. A log line on swallowed cache errors would make both failure modes diagnosable.
