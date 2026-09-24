---
kind: chore
status: open
area: [src-tauri/src/updates/mod.rs, src-tauri/src/lib.rs]
tags: [macos]
---

# Verify the first in-app update

0.12.0 (2026-09-24) is the first release with the self-updater (ADR
0053). Its feed is live at `https://torchsnap.app/updates/latest.json`.
No installed app has updated through that feed yet.

With the next release, update an installed 0.12.0 in-app and check:

- the update window lists the notes of the new version;
- "Install and Restart" replaces the app without a Gatekeeper prompt;
- after the restart the launcher shows once. This is the only hands-on
  check of `73ca3d3` (a launcher show requested during startup used to
  be hidden again by the warm-up); a unit test covers the step order,
  and the test builds before 0.12.0 did not retry it.
