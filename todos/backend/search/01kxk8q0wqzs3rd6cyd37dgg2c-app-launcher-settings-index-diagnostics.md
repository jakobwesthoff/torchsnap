---
kind: feature
status: open
---

# App launcher: settings panel with index stats and repair tools

Requested 2026-07-15 after the missing-apps incident
(`01kxk8kxsaed0ah2q0z6eamhg2-app-discovery-fallback-directory-scan.md`):
the app-launcher gadget should get a settings surface that makes the
state of the application index visible and repairable by the user.

## Requested contents

- Index statistics (e.g. number of discovered applications).
- A "last indexed" time display. The gadget already tracks this as
  `last_refresh: Arc<AtomicI64>`
  (`src-tauri/src/gadgets/app_launcher.rs:49`); it only needs to be
  exposed.
- A button to force a re-read of the index, bypassing the 5-minute
  refresh interval (`REFRESH_INTERVAL_SECS = 300`,
  `src-tauri/src/gadgets/app_launcher.rs:44`).
- Help text describing how to repair a damaged Spotlight index when
  the numbers look implausible (discovery uses `mdfind`;
  `sudo mdutil -E /` erases and rebuilds the volume's Spotlight
  index).

## Repair knowledge for the help text (validated 2026-07-15)

All of the following was exercised on a real corrupt index (macOS
26.5.1) during the missing-apps incident
(`01kxk8kxsaed0ah2q0z6eamhg2-app-discovery-fallback-directory-scan.md`):

- **Detecting a broken store.** `mdutil -s` reporting
  "Indexing enabled." says nothing about store health — it reflects
  only the on/off setting. On the affected machine the Data-volume
  store returned zero results for every query while status showed
  enabled. Reliable health signals:
  - Compare an `mdfind` app query against the actual contents of
    `/Applications` (the plausibility check from the fallback todo).
  - The unified log: `mds` logged
    `Bad checksum on fetch attributes reply from store` when queries
    hit the corrupt store
    (`log show --predicate 'process == "mds"'`).
- **The Spotlight UI is not a valid health check.** Application
  results in the Spotlight UI come from the LaunchServices registry,
  not the volume metadata store; the UI kept finding apps that
  `mdfind` could not. Help text must not tell users "check if
  Spotlight finds it" as a diagnostic.
- **Repair command and targeting.** The rebuild must target the
  volume-group root:

      sudo mdutil -E /

  Running `mdutil -E` against `/System/Volumes/Data` directly failed
  with `Error: unable to perform operation. (-405) / Error: unknown
  indexing state.` even though the corruption lives on the Data
  volume. Status queries against the Data mount also show
  `unknown indexing state` while the rebuild is in progress.
- **Success looks unremarkable.** `mdutil -E /` prints just
  `Indexing enabled.` — that is the success output (state after
  scheduling the erase), not a no-op.
- **Verifying the rebuild.** Within minutes of the erase, a dozen
  `mdworker_shared` processes ran at 20–50% CPU and
  `mdfind "kMDItemContentType == 'com.apple.application-bundle'"`
  went from 1 to 67 results under `/Applications/`. Full-disk
  content indexing continues longer, but app bundles appeared early.
- **torchsnap-side recovery.** No app action needed: the next
  background refresh after the store is repopulated picks the apps
  up (`REFRESH_INTERVAL_SECS = 300`); an app restart forces
  immediate re-discovery. The force-reindex button requested above
  covers this without a restart.
- **Escalation options (discussed, not exercised — the plain
  rebuild sufficed):** cycling `sudo mdutil -i off /` /
  `sudo mdutil -i on /` to rewrite the store configuration;
  granting the terminal Full Disk Access; removing
  `/System/Volumes/Data/.Spotlight-V100` and restarting `mds` via
  `launchctl kickstart -k system/com.apple.metadata.mds`. Verify
  these before putting them into user-facing help text.

## Root-cause background (this incident)

The corruption traced to the macOS 26.5.1 update (installed
2026-06-19, per `/Library/Receipts/InstallHistory.plist`): the Data
volume's scan base time was reset at update time, and the store that
rebuild produced was corrupt (bad-checksum reads, zero query
results). No unclean shutdown or kernel panic was involved. Useful
context for the help text: an OS update alone can silently break the
index that app discovery depends on.
