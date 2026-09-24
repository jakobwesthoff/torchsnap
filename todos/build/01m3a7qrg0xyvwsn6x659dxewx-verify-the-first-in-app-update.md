---
kind: chore
status: open
area: [just/release.just, src-tauri/src/updates/mod.rs, src-tauri/src/lib.rs]
tags: [macos]
---

# Finish the 0.12.0 rollout and verify the first in-app update

0.12.0 is the first release with the self-updater (ADR 0053). Two
things remain after it is published:

1. **Merge torchsnap-web's `auto-updater` branch** (commit `48676de`,
   its ADR 0009) and push it. Its build downloads the latest release's
   `release.json` and fails while no release carries one, so it merges
   only after `just release-publish 0.12.0`. The deploy then serves
   `https://torchsnap.app/updates/latest.json` and shows the version in
   the hero's macOS pill. Check both after the deploy; GitHub Pages may
   serve the old file for a few minutes.
2. **Update an installed 0.12.0 to the next release in-app**, the first
   update through the real feed. Check:
   - the update window lists the notes of the new version;
   - "Install and Restart" replaces the app without a Gatekeeper
     prompt;
   - after the restart the launcher shows once. This is the only
     hands-on check of `73ca3d3` (a launcher show requested during
     startup used to be hidden again by the warm-up); a unit test
     covers the step order, the test builds before 0.12.0 did not
     retry it.

The worktrees `../torchsnap--auto-updater`, `../torchsnap-docs--auto-updater`
and `../torchsnap-web--auto-updater` and their `auto-updater` branches
can be removed once item 1 is merged.
