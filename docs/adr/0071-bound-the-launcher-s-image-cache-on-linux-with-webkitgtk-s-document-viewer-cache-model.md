# 71. Bound the launcher's image cache on Linux with WebKitGTK's document viewer cache model

Date: 2026-10-24

## Status

Accepted

## Context

With the `randomMascots` setting on, which is the default, the launcher
draws a new random mascot every time its window loses focus
(`src/mascot/useMascotVariant.ts`), from a set of 626 variants
(`src/derived/mascots.json`). So the shows bring one image after
another that the launcher has not shown before. That is how the
launcher is meant to behave, and it stays.

On Linux the launcher is a WebKitGTK web view. WebKitGTK keeps each
image it has shown in its memory cache, as a decoded copy (576 KiB for
a 384x384 mascot file) and as a GPU texture (592 KiB), up to limits of
its own. Torchsnap holds no reference to earlier images: `Mascot.tsx`
renders one `<img>` for the current variant, and the list of recently
shown mascots holds character names as strings
(`src/mascot/selection.ts`).

On a Raspberry Pi 5 (GNOME 48 on Wayland, WebKitGTK 2.54.0), 500
show/hide cycles left the web process 38.0 MiB anon and 30.5 MiB GPU
memory above its level before the cycles; the GPU memory reached
+30.4 MiB at about cycle 200 and stayed there. With five mascot images
rotating instead of new ones, nothing grew.

WebKitGTK's cache model sets what its caches keep. The documentation of
`webkit_web_context_set_cache_model` says "All web views follow the
cache model" and "The default value is
%WEBKIT_CACHE_MODEL_WEB_BROWSER". It describes the document viewer
model as "Disable the cache completely, which substantially reduces
memory usage. Useful for applications that only access a single local
file, with no navigation to other pages."

Other ways to bound the growth, measured on the same machine over 200
cycles unless noted:

- Clearing WebKit's memory cache after every hide: GPU memory
  +2.2 MiB, anon +25.9 MiB.
- A memory limit of 300 MB for the web process: the property can only
  be set when the web context is built, which wry does, so it needs a
  changed wry. The median time from show to visible content went from
  72 to 82 ms.
- Painting the mascot into one reused `<canvas>` and closing each
  decoded bitmap: anon +5.1 MiB and GPU +2.8 MiB over 500 cycles. It is
  a frontend change and applies on every platform; in one run the time
  from show to visible content reached 429 ms.

## Decision

On Linux, `FallbackLauncherPanel::init` sets the cache model
`CacheModel::DocumentViewer` through the launcher window's web view,
right after the window is built. `webkit2gtk` is a Linux dependency at
the version and feature wry uses (`=2.0.2`, `v2_38`).

The cache model is a WebKitGTK API, and the change is in the Linux path
only. macOS is unchanged and was not measured.

WebKitGTK stores the cache model for the whole process: once it is set
on one web context, every context reports it. Torchsnap's windows set
no data directory, so tauri gives them one shared web context. The
settings, welcome and update windows run with the document viewer cache
model as well.

## Consequences

On the Raspberry Pi 5, 500 show/hide cycles with the document viewer
cache model left the web process 2.5 MiB anon and 2.7 MiB GPU memory
above its level before the cycles. The median time from show to visible
content was 75 ms, against 72 ms without it; one first show took
545 ms. Over 200 cycles, two runs with it ended at +23.9 and +1.5 MiB
anon and +2.4 and +2.6 MiB GPU. About 25 MiB of the web process's anon
comes and goes between runs, also in runs that draw no new mascots;
which allocation holds it is not established.

The UI process's file pages of the frontend assets embedded in the
binary, which grew by 21.4 MiB over 500 cycles with the document viewer
cache model, are not affected by it.
