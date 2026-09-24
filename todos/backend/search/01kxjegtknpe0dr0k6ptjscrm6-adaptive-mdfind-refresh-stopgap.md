---
kind: improvement
status: needs-discussion
---

# Stopgap: adaptive mdfind refresh until the result set stabilizes

Implementation plan proposed below, not yet decided.

Scope: a small, contained change to the existing mdfind polling in the
app-launcher gadget. It is a stopgap: the
[NSMetadataQuery live-query rewrite](01kxjearrc1fx4nbddxzagvq0p-nsmetadataquery-live-discovery.md)
deletes all of the machinery added here, because its
`NSMetadataQueryDidUpdateNotification` deltas provide fast incremental
updates without polling. Implement this only if that rewrite is going
to sit for a while. Related:
[persistent app-list cache evaluation](01kxjdz6qycn1wdpawm0rd0w75-persistent-app-list-cache.md).

## Problem

macOS app discovery is a one-shot `mdfind` subprocess call
(`src-tauri/src/platform/macos/app_discovery.rs:107`, query
`kMDItemContentType == 'com.apple.application-bundle'`, plus a
`/Applications/`-path filter and `Info.plist` parsing). The
app-launcher gadget runs it once at startup (`enable()`,
`src-tauri/src/gadgets/app_launcher.rs:170`) and afterwards at most
every `REFRESH_INTERVAL_SECS = 300` (`app_launcher.rs:44`), triggered
lazily from `entries()` during searches
(`maybe_trigger_background_refresh`, `app_launcher.rs:82-123`).

Observed on 2026-07-15 (macOS 26.3.1): the machine booted at 09:46:57,
torchsnap auto-started at 09:47, and its startup discovery returned
only 82 apps — exclusively system-volume apps (Safari,
`/System/Applications`, CoreServices). All 94 third-party apps in
`/Applications` and `~/Applications` were missing because Spotlight
was not yet serving complete Data-volume results that soon after
boot. The same process later (10:06) got the full 176 apps, ruling
out permissions/TCC; the only variable was time since boot. There is
no public "index warm" signal: `mdutil -s` reported
"Indexing enabled." during the incident window as well as after
recovery.

Two properties of the current code amplified the partial result:

1. A partial `Ok` from `discover()` is indistinguishable from a
   complete one; it wholesale-replaces the cache
   (`app_launcher.rs:112-113`), so searches right after login could
   not find third-party apps. (A hard `Err` behaves better: the old
   cache is kept, `app_launcher.rs:116-119`.)
2. Every successful discovery runs `icon_cache.cleanup` against its
   result set (`app_launcher.rs:110,182`), so the 94 icons of the
   "vanished" apps were deleted as orphans and had to be re-extracted
   when the full result arrived.

Recovery took the 5-minute staleness window plus a search to trigger
it, and the refresh is asynchronous, so even the triggering search
still showed the bad list.

## Idea

While consecutive discovery results still differ, the index is
considered warming up: poll on a short interval, merge results
additively (never remove entries), and suppress icon-cache cleanup.
Once two consecutive runs return the identical result set, the state
is considered stable: fall back to the 300-second cadence and resume
today's wholesale-replace + cleanup behavior.

Cost of fast polling is negligible: a full replication of the
pipeline (mdfind + `/Applications/` filter + parsing all 176
`Info.plist` files) measured 0.6 s wall clock on this machine
(2026-07-15), and it runs on a background thread.

**Stabilize on the result set, not the count.** A count can plateau
mid-warm-up or stay equal while contents shift; comparing the set of
app identities is equally cheap and actually correct.

## Implementation plan (proposed)

All changes are confined to
`src-tauri/src/gadgets/app_launcher.rs`; the `AppDiscovery` trait and
the mdfind backend stay untouched.

### 1. Constants

```rust
/// Refresh cadence once discovery results have stabilized.
const REFRESH_INTERVAL_STABLE_SECS: i64 = 300; // today's value
/// Refresh cadence while consecutive results still differ.
const REFRESH_INTERVAL_WARMUP_SECS: i64 = 15;  // value open for discussion
/// Consecutive identical result sets required to declare stability.
const STABILITY_THRESHOLD: u8 = 2;
```

### 2. Stability state

```rust
struct Stability {
    /// Fingerprint of the previous discovery result
    /// (hash over the sorted app ids; on macOS the id is the
    /// absolute bundle path).
    last_fingerprint: Option<u64>,
    /// How many consecutive results matched `last_fingerprint`.
    consecutive: u8,
}

impl Stability {
    /// Record a result's fingerprint; returns whether the state
    /// is now stable.
    fn observe(&mut self, fingerprint: u64) -> bool { ... }
    fn is_stable(&self) -> bool { self.consecutive >= STABILITY_THRESHOLD }
}
```

Held as `Mutex<Stability>` in `AppLauncherGadget` next to the existing
`cache` / `last_refresh` / `refreshing` fields. Fingerprint = hash of
the sorted `DiscoveredApp::id` list. Known limitation: a metadata-only
change (display name edited in `Info.plist` without the path changing)
does not alter the fingerprint; whether to include `name`/`bundle_id`
in the fingerprint is an open question.

`observe` semantics: equal fingerprint increments `consecutive`
(saturating), different fingerprint resets it to 1 and stores the new
fingerprint. Consequence: after a real change (app installed or
uninstalled) the gadget drops back to warm-up cadence until two
consecutive runs agree again; this is intended, as install/uninstall
bursts settle within one or two fast polls.

### 3. Interval selection

`maybe_trigger_background_refresh` (`app_launcher.rs:82-98`) currently
compares against the fixed `REFRESH_INTERVAL_SECS`. Change the
comparison to pick the interval from the stability state. Extract the
decision into a pure function for testability:

```rust
/// Which refresh interval applies given the current stability.
fn refresh_interval(stable: bool) -> i64 { ... }
```

The trigger mechanism itself stays search-driven: `entries()` →
`maybe_trigger_background_refresh` (`app_launcher.rs:195`). See open
question on a warm-up timer thread.

### 4. Result handling (both `enable()` and the refresh thread)

Unify the two `Ok(apps)` paths (`app_launcher.rs:107-115` and
`170-186`) into one handler:

- Compute the fingerprint, call `Stability::observe`.
- **Stable** (threshold reached): today's behavior — wholesale-replace
  the cache, `extract_icons` over the full set,
  `icon_cache.cleanup` with the full key set.
- **Warm-up** (not yet stable): merge additively into the cache —
  insert new entries and replace existing ones by `id`, never remove.
  Run `extract_icons` only over entries that were actually
  added/replaced (avoids re-extracting the whole set every 15 s).
  **Skip `icon_cache.cleanup` entirely** — cleanup against a partial
  set is what deleted 94 valid icons on 2026-07-15.
- `Err` path unchanged: keep cache, log to stderr.

Extract the merge as a pure function:

```rust
/// Merge a discovery result into the cached list without removing
/// entries; returns the ids that were added or replaced.
fn merge_additive(cache: &mut Vec<DiscoveredApp>, new: Vec<DiscoveredApp>) -> Vec<String> { ... }
```

The initial `enable()` discovery is warm-up round 1 by definition
(`consecutive == 1 < STABILITY_THRESHOLD`), so startup never runs
cleanup and never publishes a shrunken list — the two amplifiers from
the incident are both gated behind stability.

### 5. Accepted trade-offs (documented behavior, not bugs)

- An uninstalled app disappears from results only when a stable
  (wholesale-replace) refresh runs: the uninstall changes the
  fingerprint → warm-up → two identical fast polls → stable replace.
  With searches ongoing that is roughly 2 × 15 s; without searches it
  waits for the next search, same as today.
- During warm-up a renamed/moved app appears under both its old and
  new path until stability is reached (old entry is never removed
  additively). Launching the old entry fails if the bundle is gone;
  `execute()` already returns an error result for that case
  (`app_launcher.rs:231-256` via `opener.open_path`).
- Between searches nothing refreshes, warm-up or not — unchanged from
  today.

## Test coverage (part of the implementation)

Pure-logic units (no discovery backend, no filesystem):

- `Stability::observe`: `[A, A]` → stable on the second observation;
  `[A, B, B]` → stable on the third; `[A, B, A, A]` → stable on the
  fourth; stable then `C` → unstable again; `consecutive` saturates
  rather than overflowing on long identical runs.
- Fingerprint: order-independent (same set in different order → same
  hash); differs on added/removed id; empty set has a fingerprint
  (empty ≠ absent — two consecutive empty results are "stable empty",
  see open question).
- `merge_additive`: adds new ids; replaces an existing id with the new
  entry (returned as changed); never removes; returns exactly the
  added/replaced ids; merging an identical list returns empty; merge
  into an empty cache equals the input.
- `refresh_interval`: warm-up vs stable value.

Gadget-level with a scripted mock `AppDiscovery` (the trait is already
object-safe and injected via `AppLauncherGadget::new`,
`app_launcher.rs:68`):

- Sequence partial(82-like) → full(176-like) → full: `entries()` never
  shrinks during warm-up, equals the full set after stability.
- Icon-cache cleanup runs only on the stable transition: use a real
  `IconCache` on a temp dir, pre-seed icon files for apps missing from
  the partial result, assert they survive warm-up refreshes and the
  orphaned ones are deleted only after the stable refresh.
- Discovery `Err` during warm-up: cache kept, stability state
  unchanged.
- Uninstall after stability: entry present until the post-warm-up
  stable replace, then gone.

## Documentation updates (part of the implementation)

- Module header of `src-tauri/src/gadgets/app_launcher.rs` (lines
  5-22 currently document the flat 5-minute refresh) — describe the
  two cadences, the stability criterion, and the
  cleanup-only-when-stable rule.
- CHANGELOG entry.

## Open questions

- `REFRESH_INTERVAL_WARMUP_SECS` value (15 s proposed; anything in the
  5–30 s range is defensible given the 0.6 s pipeline cost).
- Include `name`/`bundle_id` in the fingerprint, or ids only?
- Empty result handling: two consecutive empty results would be
  "stable empty" and wholesale-replace the cache with nothing,
  deleting all icons. Treat an empty result as never stable (refuse to
  replace a non-empty cache with an empty stable set)?
- Add a dedicated timer thread during warm-up so convergence does not
  depend on the user searching? Trade-off: fixes the
  "search once, close launcher, come back later" case, but adds
  thread-lifecycle machinery that the NSMetadataQuery rewrite deletes
  again; without it the change stays entirely inside the existing
  search-triggered flow.
