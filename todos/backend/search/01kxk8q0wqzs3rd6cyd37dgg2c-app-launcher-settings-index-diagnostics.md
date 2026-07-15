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
