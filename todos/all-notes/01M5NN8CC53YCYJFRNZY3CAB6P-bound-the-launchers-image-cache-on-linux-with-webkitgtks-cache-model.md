---
title: "Bound the launcher's image cache on Linux with WebKitGTK's cache model"
kind: improvement
component: backend
origin: investigation
tags: [linux, memory]
---
# Bound the launcher's image cache on Linux with WebKitGTK's cache model

Every launcher show draws a new random mascot (`src/mascot/useMascotVariant.ts`, the draw on blur), and WebKitGTK keeps each new image in its memory cache as a decoded copy (576 KiB) and a Skia GPU texture (592 KiB). Over 500 show/hide cycles on a Raspberry Pi 5 (GNOME 48, WebKitGTK 2.54) the web process ends about 38 MiB anon and 30 MiB GPU memory above its starting level; the growth stops after about 200 shows. It is a cache with WebKit's limits, not a leak: with five distinct images rotating nothing grows. A new random mascot on every show is the product's main gag and stays.

## Measurement

Linux memory investigation, 2026-10-24, debug and release builds behaving the same:

- today: web anon +38.0 MiB, web GPU +30.4 MiB at 500 cycles, show to visible content 72 ms median;
- with the launcher's web context set to `CacheModel::DocumentViewer`: web anon +2.5 MiB, web GPU +2.7 MiB at 500 cycles, 75 ms median (one first show of 545 ms in ten).

Clearing WebKit's memory cache on every hide bounded the GPU part but not the anon part (+25.9 MiB anon at 200 cycles). A web-process memory limit needs a wry change and made shows slower.

## Suggested change

After the launcher window is built, set the cache model through `with_webview`: `webview.inner().context().set_cache_model(CacheModel::DocumentViewer)`, with `webkit2gtk = "=2.0.2"` as a Linux dependency (the version wry pins). Linux only; macOS's WKWebView has no such setting.

## Before it lands

- The measurement never looked at the screen. In the follow-up memory review, two other WebKit switches (hardware acceleration off) saved a lot of memory only because the launcher no longer painted at all, while the control API and the page's content-visible state reported a normal show. Confirm with screenshots that the card and a new mascot render on consecutive shows with the cache model set, before trusting the figures above.
- tauri-runtime-wry shares one web context among all webviews without a data directory, so the settings and welcome windows lose their memory cache too. Check that they still load and behave as before.
- A test that the launcher's context gets the cache model.
- A measurement on the Linux testbed after the change (`just testbed-bench-memory --cycles`).
