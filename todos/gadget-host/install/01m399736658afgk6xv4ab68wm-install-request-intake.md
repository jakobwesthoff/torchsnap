---
kind: feature
status: open
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
---

# Install request intake: one pipeline for every way a gadget archive arrives

Design settled on 2026-09-24. Implementation steps, tests and module
layout are in the plan (steps 3 to 12 and 16).

## Why

Gadget archives reach the app from several places, each with its own
delivery mechanism and timing:

| Origin | Delivery | Platforms |
|---|---|---|
| Settings file picker | `install_queue_submit` | all |
| Settings drop zone | `install_queue_submit` | all |
| OS "open file" | `RunEvent::Opened { urls }` | macOS |
| Command line, first instance | `std::env::args()` in `setup` | Linux, Windows, macOS when the binary runs directly |
| Command line, second instance | single-instance plugin callback `(app, args, cwd)` | all |
| `torchsnap://` link (parked) | `RunEvent::Opened` on macOS, argv on Linux | all |

All of them feed one queue, so buffering, staging, review and install
exist once.

## Decisions

- **Every origin goes through the queue and the review**, including
  the in-app picker and drop zone. `install_gadget_archive` goes away.
- **Stages:** receive (per-origin adapter) → normalize (`file://`
  URLs, relative paths with cwd, `*.torchsnap` only, canonicalize) →
  stage (capped copy into app-owned space, parsed there) → queue
  (dedup by canonical path while pending) → present (settings window
  on Gadgets) → review → install or replace from the staged copy.
- **Staging** lives in `<app_cache_dir>/install-staging/<ulid>.torchsnap`,
  capped at 16 MiB, swept at startup. The review and the install both
  work on the staged copy, so what the user approved is what lands.
- **Presentation is pull plus notify.** The frontend pulls the queue
  on mount and re-pulls on `install-queue-changed`. A request that
  arrives while the settings window does not exist is still there
  when it opens. This avoids the lost-event problem in
  `todos/backend/settings/01kwh386ce7p2p40xksx8gph3j-open-gadget-settings-event-has-no-listener.md`.
- **The queue exists before the app is built.** tao forwards
  `RunEvent::Opened` without a queue, and on a cold launch it probably
  arrives before `setup`. Creating the queue in `run()` and sharing it
  with the run-loop closure makes the order irrelevant.
- **Several files** are reviewed one after another in arrival order.
  Results collect into one summary with a single restart prompt.
- **Already installed ids are replaced** on confirmation, keeping the
  gadget's data. This needs the pending-change tracking from
  `todos/gadget-host/install/01kwh2e8mne5bd05tpb4paacwd-install-uninstall-blocked-until-restart.md`,
  which the plan implements first.
- **Undo until restart.** Every install and replace in the result
  banner can be undone; replace keeps the previous archive as
  `.<id>.torchsnap.prev` for that.
- **Requests before `setup` are buffered.** The queue only collects
  raw inputs until `setup` starts it with its dependencies, since a
  Finder cold start probably delivers `Opened` before `setup`.
- **Confirm re-checks the decision** and fails the request if another
  install or an uninstall changed it since the review.
- **No control-socket origin** until the socket has authentication
  (`todos/backend/control/01kwh4j9bptrayf451yzd2145v-control-socket-no-auth.md`).
- **The review dialog is part of this pipeline from the start.** No
  separate minimal confirm step gets built.

## Constraints from the current code

- The settings window is built on demand, destroyed on close, and
  shown after `react-ready` (`show_settings_window`,
  `show_auxiliary_window_main_thread` in `src-tauri/src/lib.rs`).
- Install is restart-based. The `GadgetHost` slot list is frozen after
  setup; `gadget_sources()` is a snapshot of it.
- The app has no Dock icon (activation policy `Accessory`), so there
  is no "drop on Dock" entry point.
