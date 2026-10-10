---
title: "Check removed apps on disk before dropping them from the app cache"
kind: improvement
component: gadgets
origin: incident
---

# Check removed apps on disk before dropping them from the app cache

A background refresh replaces the in-memory app cache wholesale with
whatever discovery returns. When Spotlight silently drops entries, apps
that are still installed vanish from the launcher. Checking the bundle path
of each vanished entry tells a real uninstall apart from an index dropout.

## Proposal

Discussed 2026-07-15. A cheap filesystem check can tell a genuine
uninstall from an index dropout, because `DiscoveredApp` carries the bundle
path (`src-tauri/src/platform/app_discovery.rs:27-50`):

1. Diff the current cache against the fresh discovery result.
2. For every entry that disappeared, `stat` its bundle path.
   - Path gone: a genuine uninstall (or move). Drop the entry.
   - Path still exists: an index dropout. Retain the entry and log a
     warning through the structured logging pipeline.

An app that was moved or renamed loses its old path and is correctly
dropped. Its new location is only found once the index (or a fallback
scan) picks it up. The incident behind this is described in the todo
titled "Add a directory-scan fallback when Spotlight results are
implausible".

## Relations

- Relates to: [Add a directory-scan fallback when Spotlight results are implausible](01KXK8KXSAED0AH2Q0Z6EAMHG2-add-a-directory-scan-fallback-when-spotlight-results-are-implausible.md), the incident this check guards against.
- Relates to: [Route app discovery diagnostics into the structured logging system](01KXK8KXSBGMZS1SZ3SA1MWJ3E-route-app-discovery-diagnostics-into-the-structured-logging-system.md), the warning for retained entries goes through that pipeline.
