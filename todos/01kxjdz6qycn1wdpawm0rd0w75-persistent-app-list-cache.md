# Evaluate persistent storage/cache for discovered applications

Status: open — to be discussed and decided.

## Problem

The app-launcher gadget holds the discovered application list only in
memory (`cache: Arc<RwLock<Vec<DiscoveredApp>>>`,
`src-tauri/src/gadgets/app_launcher.rs:48`). Every app start re-runs
Spotlight discovery from scratch via
`mdfind "kMDItemContentType == 'com.apple.application-bundle'"`
(`src-tauri/src/platform/macos/app_discovery.rs:107`).

When torchsnap auto-starts right after login, Spotlight's Data-volume
index may not be serving complete results yet. Observed on 2026-07-15
(macOS 26.3.1): boot at 09:46:57, torchsnap started 09:47, its startup
discovery returned 82 apps — only system-volume apps (Safari,
`/System/Applications`, CoreServices). All 94 third-party apps in
`/Applications` and `~/Applications` were missing. A background refresh
at 10:06 found the full 176. The same process got both results, ruling
out permissions/TCC; the only variable was time since boot.

Consequences of the partial result:

- The 82-app list was published as authoritative; searches right after
  login (prime launcher usage time) could not find third-party apps.
- `icon_cache.cleanup` deleted the 94 icons of the "vanished" apps as
  orphans; they were re-extracted on the healing refresh (cache churn,
  self-repairing).
- Recovery requires the 5-minute staleness window to elapse AND a
  search to trigger the refresh (`app_launcher.rs:82-123`), and the
  refresh is asynchronous, so the triggering search still shows the
  bad list.
- The discovery error path (`Err`) keeps the old cache, but a partial
  `Ok` result is indistinguishable from a complete one and replaces it.

## Proposal to evaluate

Persist the last-known-good application list across restarts (the
project already has `FileStorage` and SQLite storage primitives, see
ADR 0018), serve it immediately at startup, and reconcile with live
discovery once results arrive. Possibly attach a lifetime/TTL to the
persisted list.

## Open questions

- Do we want persistence at all, or is fixing startup readiness alone
  (see related options below) sufficient?
- Storage form: SQLite vs. flat file via `FileStorage`.
- Lifetime semantics: hard TTL after which the persisted list is
  discarded, vs. stale-while-revalidate (serve always, replace when a
  trusted discovery completes)?
- What counts as a "trustworthy" discovery result that may overwrite
  the persisted list (and trigger icon-cache cleanup)? A partial
  post-boot result must not.
- Should stale persisted entries be validated cheaply before serving
  (e.g. path exists check) to avoid offering apps that were
  uninstalled?
- Icon-cache cleanup coupling: today cleanup runs after every
  successful discovery; with a persisted list it should probably only
  run against a trusted/complete result.

## Related options discussed for the readiness problem (2026-07-15)

- No public "index warm" signal exists: `mdutil -s` reports only the
  persistent enabled/disabled flag ("Indexing enabled." during the
  incident window as well as after recovery).
- Cross-check mdfind results against a cheap shallow `read_dir` of
  `/Applications` and `~/Applications`; if bundles visible on disk are
  missing from mdfind output, treat the result as partial and
  retry/backoff instead of publishing it.
- Retry until two consecutive query results agree (weaker heuristic,
  no filesystem cross-check needed).
- Replace one-shot `mdfind` subprocess calls with a live
  `NSMetadataQuery` (available through the objc2 crates already used
  in `platform/macos/`): initial gathering completes with a
  did-finish-gathering notification and subsequent index updates
  stream in live, which would also replace the 5-minute polling
  refresh.

These are alternatives or complements to persistence: with a persisted
last-known-good list, startup readiness mostly stops being user-visible;
without one, a readiness gate is needed to avoid publishing partial
results.
