---
title: "Find out why the Spotlight daemons crash on macOS 27.0.1"
kind: investigation
component: platform
origin: incident
tags: [macos]
---
# Find out why the Spotlight daemons crash on macOS 27.0.1

From 2026-11-04 to 2026-11-06 `mds` and `mds_stores` crashed repeatedly on
the development machine, and Torchsnap lost its whole app list on
2026-11-05. App discovery depends on Spotlight, so it matters whether this
is a damaged index on one machine or a macOS bug that users can hit.

## Context

Machine: macOS 27.0.1, build 26A434, installed 2026-10-03, up 16 days on
2026-11-06. 75 % of the disk used, 237 GB free.

`/Library/Logs/DiagnosticReports` held 50 `mds*.ips` reports, the oldest
from 2026-11-04 00:20 (older ones may have been purged):

| Count | Process | Crash | Span |
|-------|---------|-------|------|
| 26 | `mds` | SIGSEGV in `_oidPathForOid` via `SIPersistentIDStoreGetOidPathForOid`, address `0xbad` | 2026-11-04 00:20 to 2026-11-05 16:49 |
| 21 | `mds_stores` | SIGABRT, stack buffer overflow in `createPathNodeForOid`, via `__queryRemapFieldsForStore_block_invoke_5` and `finishRegisterQuery` | 2026-11-04 02:28 to 2026-11-06 12:48 |
| 3 | `mds` | SIGABRT in `mds` itself, symbol not resolved | 2026-11-04 02:28 to 03:26 |

Both large groups crash where Spotlight turns an internal object id back
into a path. That points to bad path data in the index, or to an entry
this code cannot handle in macOS 27.0.1. The reports name no file.

In the 11 `mds_stores` crashes from 2026-11-05 14:25 on, the process died
2 to 13 minutes after launch. The launch times are launchd's crash
back-off: `launchctl print system/com.apple.metadata.mds` shows `minimum
runtime = 1200` and 112 successive crashes on 2026-11-06, so after a crash
Spotlight stays down until the next 20-minute tick. A query in that gap
may be what returned nothing to Torchsnap; nobody observed it.

Each crash signature keeps its code offsets across the reports. The
`mds_stores` report read in full had other threads inside
`_SIShutdownIndex` and `_SICloseIndex` while the faulting thread registered
a query. No report names a path or client. Nothing in the install history
lines up with the first report, and the home folder has no deep tree: 22
levels at most across 432,724 folders.

Torchsnap's own query is an unlikely trigger: 92 runs of it over 8 minutes
caused no crash. Searches for the symbol names found no public report and
nothing about macOS 27. `mds` restarted at about 13:02:45 on 2026-11-06
with no matching report, and `sudo mdutil -E /System/Volumes/Data` started
an index rebuild shortly after 13:10 that day.

## Open questions

- Do the crashes continue once the rebuild has finished? `successive
  crashes` rose from 111 to 112 at about 13:42 on 2026-11-06, while the
  rebuild may still have been running. If they continue, macOS is the
  likelier cause.
- What first damaged the index, if it is damaged? Nothing found so far.

A Feedback report to Apple is warranted either way: a stack buffer
overflow and an invalid dereference in Apple's daemons are defects of
their own, whatever the input was.

## Done when

The questions have an answer or a stated reason why they cannot get one,
and the answer has a home outside this todo.
