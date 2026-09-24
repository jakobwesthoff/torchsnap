---
kind: improvement
severity: low
status: open
area: [src-tauri/src/network/website_metadata/mod.rs]
---

# Website-metadata negative cache grows without bound

## Problem
`negative_cache` is a `Mutex<HashMap<String, Instant>>` that
only ever gains entries during normal operation:

- `record_negative` inserts on every failed fetch
  (`src-tauri/src/network/website_metadata/mod.rs:450-455`);
- `is_negatively_cached` checks expiry but never removes expired
  entries (`mod.rs:437-447`);
- the retention thread cleans the SQLite cache and favicon files
  but never touches the negative cache (`mod.rs:323-343`);
- the only removal path is the manual "clear cache" settings
  action (`mod.rs:419-430`).

Callers feed it user-typed input: a lookup consumer that queries
per keystroke (the open-url style flow) produces a stream of
nonsense intermediate domains (`e`, `ex`, `exa`, …— each
syntactically valid per `validate_domain`), each failing DNS and
each depositing a permanent entry. Expired entries also stay in
the map forever; after the 30-minute TTL they are dead weight
that still occupies memory and participates in every hash
lookup.

## Impact
Slow, unbounded memory growth over a long session. Entries are
small (domain string + `Instant`), so this is hygiene rather
than an outage risk — but "cleared on app restart" is the only
real bound.

## Suggested fix
Evict opportunistically: drop the entry inside
`is_negatively_cached` when it is found expired, and let the
existing retention loop sweep the map (`retain(|_, t|
t.elapsed() < NEGATIVE_CACHE_TTL)`) alongside the SQLite
eviction. A size cap is probably unnecessary once expired
entries actually leave.
