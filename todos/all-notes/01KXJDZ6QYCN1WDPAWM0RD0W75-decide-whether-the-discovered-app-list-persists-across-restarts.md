---
title: "Decide whether the discovered app list persists across restarts"
kind: investigation
component: gadgets
status: needs-discussion
origin: incident
---

# Decide whether the discovered app list persists across restarts

The app launcher keeps its discovered application list only in memory and
re-runs Spotlight discovery at every start. Right after login the index can
still be partial, and on 2026-07-15 such a partial list was published as
authoritative. Whether to persist a last-known-good list, and how to tell a
trustworthy discovery from a partial one, is still to be decided.

## Problem

The app-launcher gadget holds the discovered application list only in
memory (`cache: Arc<RwLock<Vec<DiscoveredApp>>>`,
`src-tauri/src/gadgets/app_launcher.rs:48`). Every app start re-runs
Spotlight discovery from scratch via
`mdfind "kMDItemContentType == 'com.apple.application-bundle'"`
(`MdfindDiscovery::discover` in `src-tauri/src/platform/macos/app_discovery.rs`).

When torchsnap auto-starts right after login, Spotlight's Data-volume
index may not be serving complete results yet. Observed on 2026-07-15
(macOS 26.3.1): boot at 09:46:57, torchsnap started at 09:47, and its
startup discovery returned 82 apps, only system-volume apps (Safari,
`/System/Applications`, CoreServices). All 94 third-party apps in
`/Applications` and `~/Applications` were missing. A background refresh
at 10:06 found the full 176. The same process got both results, which
rules out permissions and TCC. The only variable was the time since boot.

Consequences of the partial result:

- The 82-app list was published as authoritative, so searches right after
  login (the moment of prime launcher usage) could not find third-party
  apps.
- `icon_cache.cleanup` deleted the 94 icons of the "vanished" apps as
  orphans. They were re-extracted on the healing refresh (cache churn,
  self-repairing).
- Recovery requires the 5-minute staleness window to elapse AND a search
  to trigger the refresh (`app_launcher.rs:82-123`). The refresh is
  asynchronous, so the triggering search still shows the bad list.
- The discovery error path (`Err`) keeps the old cache, but a partial
  `Ok` result is indistinguishable from a complete one and replaces it.

## Proposal to evaluate

Persist the last-known-good application list across restarts (the
project already has `FileStorage` and SQLite storage primitives, see
ADR 0018), serve it immediately at startup, and reconcile with live
discovery once results arrive. Possibly attach a lifetime or TTL to the
persisted list.

## Open questions

- Do we want persistence at all, or is fixing startup readiness alone
  (see the related options below) sufficient?
- Storage form: SQLite or a flat file via `FileStorage`.
- Lifetime semantics: a hard TTL after which the persisted list is
  discarded, or stale-while-revalidate (always serve, replace when a
  trusted discovery completes)?
- What counts as a "trustworthy" discovery result that may overwrite
  the persisted list (and trigger icon-cache cleanup)? A partial
  post-boot result must not.
- Should stale persisted entries be validated cheaply before serving
  (e.g. a path-exists check) to avoid offering apps that were
  uninstalled?
- Icon-cache cleanup coupling: today cleanup runs after every
  successful discovery. With a persisted list it should probably run only
  against a trusted, complete result.

## Related options discussed for the readiness problem (2026-07-15)

- No public "index warm" signal exists. `mdutil -s` reports only the
  persistent enabled or disabled flag ("Indexing enabled." during the
  incident window as well as after recovery).
- Cross-check mdfind results against a cheap shallow `read_dir` of
  `/Applications` and `~/Applications`. If bundles visible on disk are
  missing from the mdfind output, treat the result as partial and
  retry or back off instead of publishing it.
- Retry until two consecutive query results agree. This is a weaker
  heuristic and needs no filesystem cross-check.
- Replace one-shot `mdfind` subprocess calls with a live
  `NSMetadataQuery` (available through the objc2 crates already used
  in `platform/macos/`). The initial gathering completes with a
  did-finish-gathering notification, and later index updates stream in
  live. This would also replace the 5-minute polling refresh.

These are alternatives or complements to persistence. With a persisted
last-known-good list, startup readiness mostly stops being user-visible.
Without one, a readiness gate is needed to avoid publishing partial
results.
