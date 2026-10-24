---
title: "Measure the launcher's memory over show/hide cycles on macOS"
kind: investigation
component: project
origin: investigation
tags: [macos, memory]
---
# Measure the launcher's memory over show/hide cycles on macOS

On Linux every launcher show caches one new mascot image in WebKit (decoded copy plus GPU texture) until WebKit's limits stop it, about 68 MiB over 500 shows on a Raspberry Pi 5 (Linux memory investigation, 2026-10-24). The trigger, a new random mascot per show, is platform code, so macOS may do the same. Nobody has measured it: the macOS app was not run. A first attempt on 2026-10-24 stopped before any measurement (see *State*).

## What to measure

- `tools/bench-memory` with a long show/dismiss phase on macOS. `--cycles N` exists on Linux only, so this needs the option on macOS or an equivalent.
- Whether WKWebView's WebContent process grows per new mascot and where it levels off, and whether the mascot assets embedded in the binary show up in the app's footprint (on Linux they did, as up to 25 MiB of file pages, the size of the 626 384 px mascot files).
- A run with a canvas renderer for the mascot (`fetch`, `createImageBitmap`, `drawImage` into one reused `<canvas>`, `ImageBitmap.close()`), which on Linux held the web process at +5.1 MiB anon and +2.8 MiB GPU over 500 shows. On Linux it spread the time to visible content more (72 to 429 ms in one run against 62 to 101 ms today), so time the shows too.
- A figure counts only for a launcher seen painted on screen: screenshots during the run that show the card with its mascot, and a different mascot at consecutive cycles.

## State (2026-10-24)

A plan was written and reviewed; no code was committed and nothing ran. Its main points, for whoever picks this up:

- Part 1, code: `--cycles N` and `--timing-cycles N` on macOS, with `tools/bench-memory`'s macOS scenario unchanged when the options are absent (pinning tests). The macOS sampler cannot run across the cycles as Linux's does: `memsnap sample` is `footprint --sample` under a bash wrapper, and `footprint` stopped with SIGTERM leaves a JSON file cut off at 4096 bytes (SIGINT gives a complete file). So on macOS the cycles run in chunks after the scenario's sampler has finished, with a `memsnap snapshot` every 10 cycles (`after K cycles`). `tools/bench-memory-cycles` gets `--first-cycle K` so cycle numbers run 1..N across chunks.
- Part 2, measurement: release build through `tools/bench-memory`; arms default and canvas, 3 runs each, rotated, `--cycles 200 --timing-cycles 10`; report WebContent and app footprint at start, after the scenario and at cycles 50, 100 and 200, the per-cycle growth late in the run, and show times. The canvas renderer is uncommitted instrumentation behind a setting and is reverted afterwards.

## Preconditions on the Mac

- The installed Torchsnap must be quit (`pgrep -x torchsnap` empty). The benchmark's release build has the same bundle id, `app.torchsnap`, so it writes into the installed app's data: back up every `~/Library/**/app.torchsnap*` entry the runs can touch (at least Application Support, whose `settings.json` bench-memory edits, WebKit, Caches, Preferences, HTTPStorages, Saved Application State) before the first run, restore each exactly after the last, and remove any such entry the runs created.
- `screencapture` must work from the terminal the benchmark runs in. On 2026-10-24 it failed with "could not create image from display" although Ghostty was listed under Screen Recording. `tccd` charged the capture to Ghostty and logged "Failed to match existing code requirement for subject com.mitchellh.ghostty and service kTCCServiceScreenCapture": the stored entry no longer matched the running Ghostty. Fix: remove Ghostty from System Settings, Privacy & Security, Screen & System Audio Recording, add it again, and restart Ghostty if the capture still fails (a restart ends every session running in it, tmux included). Check with `screencapture -x <file>` before starting.

## Context

The only macOS data so far is ADR 0020's slight upward creep of the WebContent process across the benchmark's dismiss phases (22, 26, 37, 39 MB). The Linux data and the canvas renderer's on-screen checks are in the Torchsnap Linux memory investigation of 2026-10-24 and in `docs/research/linux-launcher-memory.md` on the `linux-testbed` branch.
