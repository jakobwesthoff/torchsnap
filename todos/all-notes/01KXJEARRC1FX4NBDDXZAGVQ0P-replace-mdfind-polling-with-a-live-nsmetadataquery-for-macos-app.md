---
title: "Replace mdfind polling with a live NSMetadataQuery for macOS app discovery"
kind: feature
component: platform
status: needs-discussion
origin: incident
---

# Replace mdfind polling with a live NSMetadataQuery for macOS app discovery

macOS app discovery is a one-shot `mdfind` call, which the app launcher
repeats at most every five minutes, triggered by searches. A live
`NSMetadataQuery` would converge as the index warms up and would remove
the polling. The implementation plan below is proposed, not yet discussed
or decided.

Related: a persisted last-known-good list and this live query compose.
The persisted list covers the instant between app start and the query's
first gathering results.

## Problem

macOS app discovery is a one-shot `mdfind` subprocess call
(`src-tauri/src/platform/macos/app_discovery.rs:107`, query
`kMDItemContentType == 'com.apple.application-bundle'`), pulled by the
app-launcher gadget at startup and then at most every 5 minutes,
triggered only by searches
(`src-tauri/src/gadgets/app_launcher.rs:44,82-123`).

Observed on 2026-07-15 (macOS 26.3.1): the machine booted at 09:46:57,
torchsnap auto-started at 09:47, and its startup discovery returned
only 82 apps: exclusively system-volume apps (Safari,
`/System/Applications`, CoreServices). All 94 third-party apps in
`/Applications` and `~/Applications` were missing, because Spotlight was
not yet serving complete Data-volume results that soon after boot. The
partial list was published as authoritative, and the icon cache deleted
the 94 "orphaned" icons. A background refresh at 10:06 found the full
176 apps and re-extracted the icons. The same process got both results,
which rules out permissions and TCC. The only variable was the time since
boot. There is no public "index warm" signal to wait on: `mdutil -s`
reported "Indexing enabled." during the incident window as well as after
recovery.

## Why NSMetadataQuery

`NSMetadataQuery` is the framework-level interface to the same Spotlight
index, but as a **live query** (per Apple's documentation, linked from the
objc2 bindings). It runs an initial gathering phase that ends with
`NSMetadataQueryDidFinishGatheringNotification`. Afterwards it keeps
posting `NSMetadataQueryDidUpdateNotification` with added, changed and
removed item deltas as the index changes.

This dissolves the boot race instead of working around it. Results that
Spotlight discovers late arrive as update notifications, so the launcher's
list converges automatically. It also makes app installs and uninstalls
appear within seconds, and it removes the 5-minute polling refresh
entirely.

## Validated technical grounding (all against the locked dependency set)

`objc2-foundation` 0.3.2 is already a macOS dependency
(`src-tauri/Cargo.toml:105`), and its `NSMetadata` and
`NSMetadataAttributes` modules ship everything needed. Currently only the
`NSString`, `NSData`, `NSGeometry` and `NSURL` features are enabled. The
following must be added: `NSMetadata`, `NSMetadataAttributes`,
`NSPredicate`, `NSArray`, `NSNotification`, `NSOperation`, and `block2`
(the `block2` crate itself is already in `Cargo.lock` transitively).

Verified present in the 0.3.2 bindings
(`src/generated/NSMetadata.rs`, `src/generated/NSMetadataAttributes.rs`):

- `NSMetadataQuery` with `setPredicate:`, `setSearchScopes:`,
  `setOperationQueue:`, `startQuery`, `stopQuery`, `isGathering`,
  `enableUpdates`, `disableUpdates`, `results`.
- Notifications: `NSMetadataQueryGatheringProgressNotification`,
  `NSMetadataQueryDidFinishGatheringNotification`,
  `NSMetadataQueryDidUpdateNotification`.
- Update delta keys in the DidUpdate userInfo:
  `NSMetadataQueryUpdateAddedItemsKey`,
  `NSMetadataQueryUpdateChangedItemsKey`,
  `NSMetadataQueryUpdateRemovedItemsKey`.
- Search scopes: `NSMetadataQueryLocalComputerScope`,
  `NSMetadataQueryIndexedLocalComputerScope`.
- `NSMetadataItem` with `valueForAttribute:` and the attribute keys
  `NSMetadataItemPathKey`, `NSMetadataItemDisplayNameKey`,
  `NSMetadataItemContentTypeKey`, `NSMetadataItemCFBundleIdentifierKey`.
- `NSMetadataQuery` is not marked main-thread-only in these bindings.
  `setOperationQueue:` directs notification delivery to an
  `NSOperationQueue`, which avoids manual run-loop management on a
  dedicated thread.

## Architectural change: pull to push

The current `AppDiscovery` trait is pull-based
(`src-tauri/src/platform/app_discovery.rs:58`):
`fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>>` plus
`fn icon(...)`. A live query is push-based, so the platform API must
change shape rather than being worked around.

Proposed trait shape (to be discussed):

```rust
/// Event stream from a discovery backend.
pub enum AppDiscoveryEvent {
    /// Initial gathering finished; the snapshot is complete as far
    /// as the platform index is concerned.
    SnapshotComplete(Vec<DiscoveredApp>),
    /// Incremental change after the initial snapshot.
    Changed {
        added: Vec<DiscoveredApp>,
        changed: Vec<DiscoveredApp>,
        removed: Vec<String>, // DiscoveredApp::id
    },
}

pub trait AppDiscovery: Send + Sync {
    /// Start the backend and deliver events to `listener` for the
    /// lifetime of the discovery object.
    fn start(&self, listener: Box<dyn Fn(AppDiscoveryEvent) + Send + Sync>)
        -> anyhow::Result<()>;

    fn icon(&self, app: &DiscoveredApp) -> anyhow::Result<Option<DynamicImage>>;
}
```

Alternative considered: keep `discover()` and add an optional watch API,
so the fallback platform stays trivial. This draft rejects it, because two
parallel code paths in the gadget (poll and push) are exactly the
complexity the change should remove. Instead, the fallback implementation
emits one `SnapshotComplete(vec![])` and never calls the listener again.
Open for discussion.

## Implementation plan (proposed)

### 1. Cargo features

Add `NSMetadata`, `NSMetadataAttributes`, `NSPredicate`, `NSArray`,
`NSNotification`, `NSOperation` and `block2` to the `objc2-foundation`
feature list in `src-tauri/Cargo.toml`.

### 2. `MetadataQueryDiscovery` (new macOS backend)

New file replacing the mdfind backend in
`src-tauri/src/platform/macos/app_discovery.rs`:

- Build the query on start:
  - `NSPredicate` from
    `kMDItemContentType == 'com.apple.application-bundle'` (the same
    predicate as today's mdfind call).
  - Search scope `NSMetadataQueryLocalComputerScope`. Keep the existing
    Rust-side path filter (`is_in_applications_dir`, path contains
    `/Applications/`), since scopes select volumes, not directories. Open
    question: `IndexedLocalComputerScope` instead?
  - Create a dedicated `NSOperationQueue` and `setOperationQueue:` so
    notifications arrive off the main thread without a run loop.
- Register block-based `NSNotificationCenter` observers (via `block2`),
  scoped to the query object, for DidFinishGathering and DidUpdate.
- On **DidFinishGathering**: call `disableUpdates`, snapshot `results()`
  (an array of `NSMetadataItem`), map each item to a `DiscoveredApp`, call
  `enableUpdates`, and emit `SnapshotComplete(apps)`. The disable and
  enable bracket around result enumeration follows the pattern the API
  requires, since updates would otherwise mutate the proxy array during
  iteration.
- On **DidUpdate**: read the three delta arrays from userInfo, map them,
  and emit `Changed { added, changed, removed }`.
- Item mapping: read `NSMetadataItemPathKey` for the path and ID. For
  display name and bundle ID there are two options (open question below):
  metadata attributes (`NSMetadataItemDisplayNameKey`,
  `NSMetadataItemCFBundleIdentifierKey`), or keeping today's `Info.plist`
  parse with its name resolution order (`CFBundleDisplayName`, then
  `CFBundleName`, then filename, `app_discovery.rs:74-83`). Note that
  `NSMetadataItemDisplayNameKey` values may carry the localized, `.app`-less
  display string. Whether it matches the current plist-derived names must
  be verified during implementation before choosing.
- Lifetime: the query object and observers are retained by the discovery
  struct for the app's lifetime, with `stopQuery` and observer removal on
  `Drop`.
- If `startQuery` returns `false`, return an error from `start()` so the
  gadget can log it. Open question: fall back to a one-shot mdfind in that
  case?
- The `icon()` implementation is unchanged (`nsworkspace_icon_for_file`).

### 3. Gadget integration (`src-tauri/src/gadgets/app_launcher.rs`)

- Delete `REFRESH_INTERVAL_SECS`, `last_refresh`, `refreshing` and
  `maybe_trigger_background_refresh`. Polling is gone.
- `enable()` calls `discovery.start(listener)`. The listener updates
  `cache: Arc<RwLock<Vec<DiscoveredApp>>>`:
  - `SnapshotComplete`: replace the cache, then run icon extraction and
    `icon_cache.cleanup` (the existing `extract_icons` helper) on the
    complete set.
  - `Changed`: patch the cache (insert or replace by id, remove by id),
    and extract icons only for added or changed entries. **No cleanup on
    deltas.** Cleanup only ever runs against a complete snapshot, so a
    partial index state can never delete valid icons again (the failure
    amplifier observed on 2026-07-15).
- Icon extraction stays off the notification thread. Spawn a worker,
  because `extract_icons` is the slow phase. The two-phase publish in
  today's `enable()` already establishes this pattern.
- `entries()` just reads the cache, as today.

### 4. Fallback platform (`src-tauri/src/platform/fallback/app_discovery.rs`)

Implement the new trait: emit `SnapshotComplete` of whatever the current
`discover()` stub returns (an empty list), and never emit again.

### 5. Out of scope

The settings-discovery path (`src-tauri/src/platform/settings_discovery.rs`
and its macOS implementation) is a separate trait and keeps its current
mechanism.

## Test coverage (to be part of the implementation)

- **Pure-logic unit tests** (no AppKit objects): extract the path filter,
  snapshot patching (apply added, changed and removed to a Vec) and name
  resolution into plain functions. Cases: empty delta; removal of an id
  not in the cache; add of an id already present (replace, not duplicate);
  paths outside `/Applications/` filtered from snapshots and deltas; an app
  removed and then re-added.
- **Gadget tests with a scripted mock discovery** implementing the new
  trait. Assert that `entries()` reflects `SnapshotComplete` and `Changed`
  sequences. Assert that icon-cache `cleanup` is invoked after a snapshot
  but not after deltas. Assert that a delta arriving before any snapshot
  does not panic.
- **macOS integration test** (`#[cfg(all(test, target_os = "macos"))]`,
  the same gating as the existing tests in
  `src-tauri/src/platform/macos/app_resolver.rs:39`): construct the real
  query, start it, wait for DidFinishGathering with a timeout, and assert
  at least one result under an Applications directory. Caveat: it depends
  on the host's Spotlight state, like the existing
  `app_path_for_identifier_finds_finder` test depends on Finder being
  installed.
- **Manual verification**: quit and relaunch the app right after a reboot.
  The launcher list should converge to the full set without interaction as
  the index warms.

## Documentation updates (to be part of the implementation)

- Rewrite the module header of `src-tauri/src/platform/macos/app_discovery.rs`
  (it currently describes mdfind) and the trait docs in
  `src-tauri/src/platform/app_discovery.rs` (they document the pull
  contract and name mdfind at line 12).
- Update the app-launcher module header (`src-tauri/src/gadgets/app_launcher.rs:5-22`,
  which currently documents the 5-minute refresh).
- CHANGELOG entry.
- Decide whether this warrants an ADR. The project records decisions as
  ADRs, e.g. 0018 and 0035.

## Open questions

- Trait shape: a callback listener as sketched, or a channel
  (`Receiver<AppDiscoveryEvent>`)?
- Metadata attributes or `Info.plist` parsing for name and bundle ID (see
  the item-mapping note above)?
- Scope: `NSMetadataQueryLocalComputerScope` or
  `NSMetadataQueryIndexedLocalComputerScope`?
- Fallback if `startQuery` fails: error only, or keep the mdfind one-shot as
  a degraded mode?
- Use `NSMetadataQueryGatheringProgressNotification` to publish partial
  results during the initial gathering (faster first paint, but it
  reintroduces partial-state handling), or wait for DidFinishGathering?
- Interaction with the persistent-cache todo: if a persisted list is
  adopted, it is served until the first `SnapshotComplete` replaces it.

## Relations

- Relates to: [Decide whether the discovered app list persists across restarts](01KXJDZ6QYCN1WDPAWM0RD0W75-decide-whether-the-discovered-app-list-persists-across-restarts.md), a persisted last-known-good list and this query compose.
