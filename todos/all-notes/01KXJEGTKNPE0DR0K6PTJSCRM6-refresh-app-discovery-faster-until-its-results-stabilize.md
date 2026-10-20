---
title: "Refresh app discovery faster until its results stabilize"
kind: improvement
component: gadgets
status: needs-discussion
origin: incident
---

# Refresh app discovery faster until its results stabilize

The app launcher polls macOS app discovery every five minutes, so a
partial result after login stays visible until the next poll. This todo
proposes a stopgap that polls faster while consecutive results still
differ. It is not decided yet. Implement it only if the NSMetadataQuery
rewrite will sit for a while, because that rewrite deletes all of this
machinery.

Scope: a small, contained change to the existing mdfind polling in the
app-launcher gadget. It is a stopgap. The NSMetadataQuery live-query
rewrite deletes all of the machinery added here, because its
`NSMetadataQueryDidUpdateNotification` deltas provide fast incremental
updates without polling.

## Problem

macOS app discovery is a one-shot `mdfind` subprocess call
(`MdfindDiscovery::discover` in `src-tauri/src/platform/macos/app_discovery.rs`, query
`kMDItemContentType == 'com.apple.application-bundle'`, plus a
`/Applications/`-path filter, an embedded-helper filter and `Info.plist`
parsing). The app-launcher
gadget runs it once at startup (`enable()`,
`src-tauri/src/gadgets/app_launcher.rs:170`) and afterwards at most every
`REFRESH_INTERVAL_SECS = 300` (`app_launcher.rs:44`). It is triggered
lazily from `entries()` during searches
(`maybe_trigger_background_refresh`, `app_launcher.rs:82-123`).

Observed on 2026-07-15 (macOS 26.3.1): the machine booted at 09:46:57,
torchsnap auto-started at 09:47, and its startup discovery returned only
82 apps: exclusively system-volume apps (Safari, `/System/Applications`,
CoreServices). All 94 third-party apps in `/Applications` and
`~/Applications` were missing, because Spotlight was not yet serving
complete Data-volume results that soon after boot. The same process later
(10:06) got the full 176 apps, which rules out permissions and TCC. The
only variable was the time since boot. There is no public "index warm"
signal: `mdutil -s` reported "Indexing enabled." during the incident
window as well as after recovery.

Two properties of the current code amplified the partial result:

1. A partial `Ok` from `discover()` is indistinguishable from a complete
   one. It wholesale-replaces the cache
   (`app_launcher.rs:112-113`), so searches right after login could
   not find third-party apps. (A hard `Err` behaves better: the old
   cache is kept, `app_launcher.rs:116-119`.)
2. Every successful discovery runs `icon_cache.cleanup` against its
   result set (`app_launcher.rs:110,182`), so the 94 icons of the
   "vanished" apps were deleted as orphans and had to be re-extracted
   when the full result arrived.

Recovery took the 5-minute staleness window plus a search to trigger it.
The refresh is asynchronous, so even the triggering search still showed
the bad list.

## Idea

While consecutive discovery results still differ, the index is considered
warming up. Poll on a short interval, merge results additively (never
remove entries), and suppress icon-cache cleanup. Once two consecutive runs
return the identical result set, the state is considered stable. Then fall
back to the 300-second cadence and resume today's wholesale-replace and
cleanup behavior.

Cost of fast polling is negligible. A full replication of the pipeline
(mdfind, the `/Applications/` filter and parsing all 176 `Info.plist`
files) measured 0.6 s wall clock on this machine (2026-07-15), and it runs
on a background thread.

**Stabilize on the result set, not the count.** A count can plateau
mid-warm-up or stay equal while the contents shift. Comparing the set of
app identities is equally cheap and actually correct.

## Implementation plan (proposed)

All changes are confined to `src-tauri/src/gadgets/app_launcher.rs`. The
`AppDiscovery` trait and the mdfind backend stay untouched.

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

Held as `Mutex<Stability>` in `AppLauncherGadget`, next to the existing
`cache`, `last_refresh` and `refreshing` fields. The fingerprint is a hash
of the sorted `DiscoveredApp::id` list. Known limitation: a metadata-only
change (a display name edited in `Info.plist` without the path changing)
does not alter the fingerprint. Whether to include `name` and `bundle_id`
in the fingerprint is an open question.

`observe` semantics: an equal fingerprint increments `consecutive`
(saturating), and a different fingerprint resets it to 1 and stores the
new fingerprint. Consequence: after a real change (an app installed or
uninstalled), the gadget drops back to warm-up cadence until two
consecutive runs agree again. This is intended, since install and
uninstall bursts settle within one or two fast polls.

### 3. Interval selection

`maybe_trigger_background_refresh` (`app_launcher.rs:82-98`) currently
compares against the fixed `REFRESH_INTERVAL_SECS`. Change the comparison
to pick the interval from the stability state. Extract the decision into a
pure function for testability:

```rust
/// Which refresh interval applies given the current stability.
fn refresh_interval(stable: bool) -> i64 { ... }
```

The trigger mechanism itself stays search-driven: `entries()` calls
`maybe_trigger_background_refresh` (`app_launcher.rs:195`). See the open
question on a warm-up timer thread.

### 4. Result handling (both `enable()` and the refresh thread)

Unify the two `Ok(apps)` paths (`app_launcher.rs:107-115` and
`170-186`) into one handler:

- Compute the fingerprint and call `Stability::observe`.
- **Stable** (threshold reached): today's behavior. Wholesale-replace the
  cache, `extract_icons` over the full set, and `icon_cache.cleanup` with
  the full key set.
- **Warm-up** (not yet stable): merge additively into the cache. Insert new
  entries and replace existing ones by `id`, and never remove. Run
  `extract_icons` only over entries that were actually added or replaced,
  which avoids re-extracting the whole set every 15 s. **Skip
  `icon_cache.cleanup` entirely.** Cleanup against a partial set is what
  deleted 94 valid icons on 2026-07-15.
- The `Err` path is unchanged: keep the cache and log to stderr.

Extract the merge as a pure function:

```rust
/// Merge a discovery result into the cached list without removing
/// entries; returns the ids that were added or replaced.
fn merge_additive(cache: &mut Vec<DiscoveredApp>, new: Vec<DiscoveredApp>) -> Vec<String> { ... }
```

The initial `enable()` discovery is warm-up round 1 by definition
(`consecutive == 1 < STABILITY_THRESHOLD`). Startup therefore never runs
cleanup and never publishes a shrunken list, so both amplifiers from the
incident are gated behind stability.

### 5. Accepted trade-offs (documented behavior, not bugs)

- An uninstalled app disappears from results only when a stable
  (wholesale-replace) refresh runs. The uninstall changes the fingerprint,
  which leads to warm-up, then two identical fast polls, then a stable
  replace. With searches ongoing that is roughly 2 x 15 s. Without
  searches it waits for the next search, same as today.
- During warm-up, a renamed or moved app appears under both its old and
  new path until stability is reached, because the old entry is never
  removed additively. Launching the old entry fails if the bundle is gone.
  `execute()` already returns an error result for that case
  (`app_launcher.rs:231-256` via `opener.open_path`).
- Between searches nothing refreshes, warm-up or not. This is unchanged
  from today.

## Test coverage (part of the implementation)

Pure-logic units (no discovery backend, no filesystem):

- `Stability::observe`: `[A, A]` is stable on the second observation;
  `[A, B, B]` is stable on the third; `[A, B, A, A]` is stable on the
  fourth; stable and then `C` is unstable again; `consecutive` saturates
  rather than overflowing on long identical runs.
- Fingerprint: order-independent (the same set in a different order gives
  the same hash); differs on an added or removed id; an empty set has a
  fingerprint (empty is not absent. Two consecutive empty results are
  "stable empty", see the open question).
- `merge_additive`: adds new ids; replaces an existing id with the new
  entry (returned as changed); never removes; returns exactly the added
  or replaced ids; merging an identical list returns empty; merging into
  an empty cache equals the input.
- `refresh_interval`: the warm-up value versus the stable value.

Gadget-level, with a scripted mock `AppDiscovery`. The trait is already
object-safe and injected via `AppLauncherGadget::new`
(`app_launcher.rs:68`):

- Sequence partial (82-like), then full (176-like), then full:
  `entries()` never shrinks during warm-up, and equals the full set after
  stability.
- Icon-cache cleanup runs only on the stable transition. Use a real
  `IconCache` on a temp dir, pre-seed icon files for apps missing from the
  partial result, and assert that they survive warm-up refreshes and that
  the orphaned ones are deleted only after the stable refresh.
- Discovery `Err` during warm-up: the cache is kept and the stability
  state is unchanged.
- Uninstall after stability: the entry is present until the post-warm-up
  stable replace, then gone.

## Documentation updates (part of the implementation)

- Module header of `src-tauri/src/gadgets/app_launcher.rs` (lines 5-22
  currently document the flat 5-minute refresh). Describe the two
  cadences, the stability criterion, and the cleanup-only-when-stable rule.
- CHANGELOG entry.

## Open questions

- `REFRESH_INTERVAL_WARMUP_SECS` value (15 s proposed; anything in the
  5-30 s range is defensible given the 0.6 s pipeline cost).
- Include `name` and `bundle_id` in the fingerprint, or ids only?
- Empty result handling: two consecutive empty results would be "stable
  empty" and would wholesale-replace the cache with nothing, deleting all
  icons. Should an empty result never count as stable, so that a
  non-empty cache is never replaced by an empty stable set?
- Add a dedicated timer thread during warm-up so convergence does not
  depend on the user searching? The trade-off: it fixes the "search once,
  close the launcher, come back later" case, but adds thread-lifecycle
  machinery that the NSMetadataQuery rewrite deletes again. Without it, the
  change stays entirely inside the existing search-triggered flow.

## Relations

- Relates to: [Replace mdfind polling with a live NSMetadataQuery for macOS app discovery](01KXJEARRC1FX4NBDDXZAGVQ0P-replace-mdfind-polling-with-a-live-nsmetadataquery-for-macos-app.md), the live query deletes the machinery this adds.
- Relates to: [Decide whether the discovered app list persists across restarts](01KXJDZ6QYCN1WDPAWM0RD0W75-decide-whether-the-discovered-app-list-persists-across-restarts.md), the same partial-result window seen from the cache side.
