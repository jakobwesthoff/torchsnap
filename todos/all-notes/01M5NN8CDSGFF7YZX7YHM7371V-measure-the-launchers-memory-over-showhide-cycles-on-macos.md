---
title: "Measure the launcher's memory over show/hide cycles on macOS"
kind: investigation
component: project
origin: investigation
tags: [macos, memory]
---
# Measure the launcher's memory over show/hide cycles on macOS

On Linux every launcher show caches one new mascot image in WebKit (decoded copy plus GPU texture) until WebKit's limits stop it, about 68 MiB over 500 shows on a Raspberry Pi 5 (Linux memory investigation, 2026-10-24). The trigger, a new random mascot per show, is platform code, so macOS may do the same. Nobody has measured it: the macOS app was not run.

## What to measure

- `tools/bench-memory` with a long show/dismiss phase on macOS. `--cycles N` exists on Linux only, so this needs the option on macOS or an equivalent.
- Whether WKWebView's WebContent process grows per new mascot and where it levels off, and whether the mascot assets embedded in the binary show up in the app's footprint (on Linux they did, as up to 25 MiB of file pages, the size of the 626 384 px mascot files).
- A run with a canvas renderer for the mascot (`fetch`, `createImageBitmap`, `drawImage` into one reused `<canvas>`, `ImageBitmap.close()`), which on Linux held the web process at +5.1 MiB anon and +2.8 MiB GPU over 500 shows. On Linux it spread the time to visible content more (72 to 429 ms in one run against 62 to 101 ms today), so time the shows too.

## Context

The only macOS data so far is ADR 0020's slight upward creep of the WebContent process across the benchmark's dismiss phases (22, 26, 37, 39 MB). The Linux data and the canvas renderer's on-screen checks are in the Torchsnap Linux memory investigation of 2026-10-24.
