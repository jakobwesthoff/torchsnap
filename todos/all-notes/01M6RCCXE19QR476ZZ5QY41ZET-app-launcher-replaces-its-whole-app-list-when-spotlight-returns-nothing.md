---
title: "App launcher replaces its whole app list when Spotlight returns nothing"
kind: bug
component: gadgets
impact: high
origin: incident
tags: [macos, error-handling]
---
# App launcher replaces its whole app list when Spotlight returns nothing

On 2026-11-06 the launcher found no applications at all on the
development machine, and the app icon cache was empty. A background
refresh had very likely received an empty but successful result from
`mdfind`, published it as the new app list and deleted every cached icon.
This happened while the Spotlight daemons on that machine were crashing
repeatedly. The list came back on its own at 13:07 on 2026-11-06. Unlike
the 2026-07-15 incidents, which lost part of the list for minutes, this
one lost all of it and lasted up to about 20 hours.

## Problem

Established by reading the code and by the on-disk evidence below
(2026-11-06). Not reproduced. Nobody observed `mdfind` returning empty
output, so that step is inferred.

`MdfindDiscovery::discover` (`src-tauri/src/platform/macos/app_discovery.rs`)
treats any exit-0 `mdfind` run as success, including one with no output.
The refresh thread in `maybe_trigger_background_refresh`
(`src-tauri/src/gadgets/app_launcher.rs`) then does three things without
checking the result:

- `extract_icons` over the empty list returns an empty key set.
- `icon_cache.cleanup` deletes every icon of the gadget as an orphan.
- The cache is replaced by the empty list and `last_refresh` advances, so
  the next attempt needs both five minutes and a search.

The `Err` arm keeps the old list and the icons. Only `Ok` is destructive.
`enable()` has the same shape at startup.

The search that triggers a refresh still sees the old list, and the
refresh publishes only after icon extraction. That is why the searches
that triggered recovery on 2026-11-06 still found nothing.

## Impact

Launching applications, the launcher's core feature, stops working with
no visible error. All icons must be extracted again afterwards. In a
packaged build the failure leaves no trace, because stderr is `/dev/null`
(checked with `lsof` on the running process).

## Evidence (2026-11-05 and 2026-11-06)

Machine: macOS 27.0.1, build 26A434, installed 2026-10-03. Torchsnap
0.15.0 from `/Applications`, running for 13 days. System up 16 days. Disk
75 % used, 237 GB free. `mdutil -s` reported "Indexing enabled." for `/`
and `/System/Volumes/Data` throughout. Times are local (CET).

- 134 of the 141 shard folders in
  `~/Library/Caches/app.torchsnap/icons/app-launcher` changed at
  2026-11-05 16:20, and the folder held 0 files around 13:06 on
  2026-11-06. The other 7 shards changed earlier and were empty too.
- 184 icon files appeared at 13:07:54 and 13:07:55 on 2026-11-06, right
  after searches in the launcher. Resident memory of the process rose
  from about 77 MB to about 97 MB. After that the apps were back.
- No icon appeared between those two points, so no refresh in that span
  returned apps. The launcher was in use during it: the frecency WAL was
  last written at 12:49 on 2026-11-06.
- `mds` and `mds_stores` had restarted at about 13:02:45 on 2026-11-06.
  No crash report matches that restart.
- From about 13:05 on, the exact query returned 435 paths, in a normal
  shell and with a minimal environment (`env -i`). A probe every 5
  seconds returned 435 on all 61 runs up to 13:10:53, without a crash.
  `/Applications` held 93 apps on disk and Spotlight listed all 93.
- `sample` of the process showed no thread stuck in discovery. It had 161
  open files, so descriptor exhaustion is not the cause.

## Spotlight crashes

`/Library/Logs/DiagnosticReports` holds 50 `mds*.ips` reports, the oldest
from 2026-11-04 00:20 (older ones may have been purged):

| Count | Process | Crash | Span |
|-------|---------|-------|------|
| 26 | `mds` | SIGSEGV in `_oidPathForOid` via `SIPersistentIDStoreGetOidPathForOid`, address `0xbad` | 2026-11-04 00:20 to 2026-11-05 16:49 |
| 21 | `mds_stores` | SIGABRT, stack buffer overflow in `createPathNodeForOid`, via `__queryRemapFieldsForStore_block_invoke_5` and `finishRegisterQuery` | 2026-11-04 02:28 to 2026-11-06 12:48 |
| 3 | `mds` | SIGABRT in `mds` itself, symbol not resolved | 2026-11-04 02:28 to 03:26 |

Both large groups crash where Spotlight turns an internal object id back
into a path, and the `mds_stores` group does so while registering a
query. That points to bad path data in the index, or to an entry this
code cannot handle in macOS 27.0.1. It is a hypothesis: the reports name
no file.

In the 11 `mds_stores` crashes checked from 2026-11-05 14:25 on, the
process died 2 to 13 minutes after launch. Launch times sit roughly a multiple of
5 minutes apart (14:15:06, 14:55:07, 17:00:02, 18:40:06, 21:30:51,
12:02:49, 12:42:50), so something periodic may be restarting it. What
that is remains unknown.

Torchsnap's own query is an unlikely trigger, given the crash-free probe
above. Searches for the symbol names found no public report and nothing
about macOS 27. Advice for similar Spotlight trouble is to rebuild the
index: `https://developer.apple.com/forums/thread/796568`,
`https://eclecticlight.co/2022/12/08/spotlight-problems-mds_stores-and-mdworker-in-trouble/`.

## Open questions

- Did `mdfind` really return empty output with exit 0 from 2026-11-05
  16:20 until 2026-11-06 13:02? No crash report falls at 16:20 (nearest:
  15:07 and 16:49), and `log show` returned nothing for these processes.
- Why did no refresh recover in about 20 hours, although the daemons
  restarted several times in that span?
- What restarted `mds` at about 13:02:45, and did the user's own action
  cause it? Asked on 2026-11-06, not answered yet.
- Is the index damaged, or is this a macOS 27.0.1 bug? A rebuild answers
  that: crashes that continue afterwards point to macOS.

The watcher on Torchsnap's child processes proved nothing. It logged no
child, but `mdfind` runs for only 18 ms and slips between its samples.

## Suggested fix

Start with the guard, test-first, before any Spotlight index rebuild,
since a rebuild can itself produce empty or partial results:

1. In `MdfindDiscovery::discover`, return `Err` when no application
   bundle remains. The existing `Err` arms in the refresh thread and in
   `enable()` then keep the list and the icons. Doing this in the macOS
   backend leaves the fallback platform's empty stub valid.
2. Add a retry limit with it. The `Err` path does not advance
   `last_refresh`, so with Spotlight down every keystroke would spawn
   `mdfind`. This is the todo "Back off background app refresh after a
   discovery failure"; land both together.
3. Regression tests in `src-tauri/src/gadgets/app_launcher.rs` with a
   scripted mock `AppDiscovery` and a real `IconCache` on a temp dir: an
   empty result keeps the entries and the icon files, and the next
   non-empty result still replaces the list. A test for the `mdfind`
   empty case needs a seam around `Mdfind::query`.

The guard covers a total loss only. Partial results and independence from
Spotlight are covered by the related todos: the on-disk check for
vanished apps, the directory-scan fallback, and the
`_LSCopyAllApplicationURLs` option in the direct API evaluation.
Logging, whose absence left this incident without a trail, is
tracked in the discovery diagnostics todo.

On the machine itself, after the guard ships:
`sudo mdutil -E /System/Volumes/Data`, then watch for new crash reports.
If they continue, send Apple a Feedback report with them.

## Done when

- An empty discovery result leaves the app list and the icon cache
  untouched, at startup and on refresh, covered by regression tests.
- Discovery retries at a bounded rate while Spotlight returns nothing.
- `CHANGELOG.md` has an entry under `[Unreleased]`.

## Relations

- Relates to: [Add a directory-scan fallback when Spotlight results are implausible](01KXK8KXSAED0AH2Q0Z6EAMHG2-add-a-directory-scan-fallback-when-spotlight-results-are-implausible.md), would have kept the list working through this incident.
- Relates to: [Check removed apps on disk before dropping them from the app cache](01KXK8KXSBGMZS1SZ3SA1MWJ3F-check-removed-apps-on-disk-before-dropping-them-from-the-app-cache.md), the general form of the guard here.
- Relates to: [Back off background app refresh after a discovery failure](01KWG2JSBMV9EHEW6XFX7DZZR9-back-off-background-app-refresh-after-a-discovery-failure.md), turning empty into `Err` makes that retry loop reachable.
- Relates to: [Refresh app discovery faster until its results stabilize](01KXJEGTKNPE0DR0K6PTJSCRM6-refresh-app-discovery-faster-until-its-results-stabilize.md), its open question on empty results is answered by this incident.
- Relates to: [Route app discovery diagnostics into the structured logging system](01KXK8KXSBGMZS1SZ3SA1MWJ3E-route-app-discovery-diagnostics-into-the-structured-logging-system.md), this incident left no log trail either.
- Relates to: [Decide whether macOS app discovery should use a direct API instead of mdfind](01KMJPBR2SAEHJHHGKH3JDSGNA-decide-whether-macos-app-discovery-should-use-a-direct-api-instead-of.md), a further correctness argument against depending on Spotlight.
