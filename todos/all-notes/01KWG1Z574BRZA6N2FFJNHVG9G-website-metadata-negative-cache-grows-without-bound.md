---
title: "Website-metadata negative cache grows without bound"
kind: improvement
component: backend
impact: low
---
# Website-metadata negative cache grows without bound

The `negative_cache` in the website-metadata service (`src-tauri/src/network/website_metadata/mod.rs`) gains an entry for every failed fetch and never drops expired ones, so it grows for the length of a session. The only removal path is the manual "clear cache" settings action, and the only real bound is an app restart.

## Problem

`negative_cache` is a `Mutex<HashMap<String, Instant>>` that only ever gains entries during normal operation:

- `record_negative` inserts on every failed fetch.
- `is_negatively_cached` checks expiry but never removes expired entries.
- The retention thread cleans the SQLite cache and the favicon files, but never touches the negative cache.
- The only removal path is the manual "clear cache" settings action.

Callers feed it user-typed input. A lookup consumer that queries per keystroke (the open-url style flow) produces a stream of intermediate domains (`e`, `ex`, `exa` and so on), each syntactically valid under `validate_domain`. Each fails DNS and deposits a permanent entry. Expired entries also stay in the map forever. After the 30-minute TTL they are dead weight that still occupies memory and takes part in every hash lookup.

## Impact

Slow, unbounded memory growth over a long session. Entries are small (a domain string and an `Instant`), so this is hygiene rather than an outage risk.

## Suggested fix

Evict opportunistically: drop an entry inside `is_negatively_cached` when it is found expired. Let the existing retention loop sweep the map as well, with `retain(|_, t| t.elapsed() < NEGATIVE_CACHE_TTL)`, alongside the SQLite eviction. A size cap is probably unnecessary once expired entries actually leave.
