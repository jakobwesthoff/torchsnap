# 73. Bound the launcher's image cache on Linux with WebKitGTK's document viewer cache model

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

On the Raspberry Pi 5, a debug build ran 200 show/hide cycles three
times with the document viewer cache model and three times without it,
in alternating order. Levels of the web process after cycle 200, above
its level before the cycles:

| | anon (MiB) | GPU memory (MiB) | decoded 384x384 images kept |
|---|---|---|---|
| without | +53.8, +34.5, +33.2 | +31.0, +29.5, +30.4 | 14, 17 (one run not counted) |
| with | +20.6, +1.1, +0.0 | +2.7, +2.8, +2.4 | 0, 0, 1 |

About 25 MiB of the web process's anon comes and goes between runs,
also in runs that draw no new mascots; which allocation holds it is not
established. It shows in the spread of both rows. Over 500 cycles, one
run with the document viewer cache model ended at +2.5 MiB anon and
+2.7 MiB GPU.

The time from show to visible content, over the ten timing cycles after
each of the six runs, had a median of 74 ms with the document viewer
cache model and 70 ms without it, a 90th percentile of 105 and 108 ms
and a maximum of 204 and 162 ms.

With and without the document viewer cache model, the same images were
on screen: a different mascot on each of three shows, the emoji grid of
`:fire` (emoji are text, not images), the favicons of the open-url and
bangs gadgets on a second query with the favicon store filled, the
preview of an image in the clipboard history, the emblem in the
settings window and the mascot in the welcome window. From a query to
the launcher reporting its results took 157, 132 and 117 ms (median of
ten) for `:fire`, a bang and a domain with the document viewer cache
model, against 142, 137 and 126 ms without it. Linux has no application icons yet, since
app discovery finds no applications there.

The UI process's file pages of the frontend assets embedded in the
binary are not affected: after 200 cycles they were 12.2 to 12.7 MiB
above their level before the cycles with the document viewer cache
model and 12.5 to 12.9 MiB without it.
