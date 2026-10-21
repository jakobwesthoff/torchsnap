---
title: "Launcher does not show after the restart that installs an update"
kind: bug
component: backend
status: needs-discussion
origin: incident
tags: [macos]
---
# Launcher does not show after the restart that installs an update

After an in-app update, Torchsnap restarts into the new version and is meant to show the launcher once as a sign it is back. Jakob confirmed on 2026-10-21 that in-app updates from 0.12.0 on work, but the launcher does not appear after that restart, also not since `ef01a26` ("Show a launcher requested during startup after the warm-up, not before"). Observed in use, not reproduced or diagnosed; needs a look before any fix.

## How it is meant to work

- `update_install` in `src-tauri/src/updates/mod.rs` writes the marker file `.show-launcher-after-update` into the app data directory (`write_show_launcher_marker`) after the plugin installed the update, then calls `app.restart()`.
- At startup, the `setup` closure in `src-tauri/src/lib.rs` takes the marker (`updates::take_show_launcher_marker`) and calls `show_launcher_window`.
- Before the frontend reported its layout, `show_launcher_window` only queues the show; `launcher_set_layout` runs it after the warm-up (`layout_steps`: Position, WarmUp, Position, Show), the order `ef01a26` fixed.

## Leads to check, none verified

- The marker never reaches the new start: a write failure only goes to stderr, and the plugin's install or `restart()` path may start the new app before the marker is written, or with a different data directory.
- The show happens but is undone at once: the app is not frontmost right after the restart, so the panel may get a `tauri://blur` and the launcher's blur dismiss (`src/launcher/visibility.ts`) hides it again.
- The queued show never fires, for example because the frontend reports its layout before `setup` queues the show, so `take_pending_show` sees nothing.

## Suggested next step

Reproduce with a test feed (start the installed build with `open --env TORCHSNAP_UPDATE_FEED=<url>`, as the project CLAUDE.md describes), log the marker write and take, the show step and any dismiss with its source, then decide the fix.
