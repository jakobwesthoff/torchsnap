---
title: "Find out why WebKitGTK paints nothing in the launcher without GPU rendering"
kind: investigation
component: platform
origin: investigation
tags: [linux, memory]
---
# Find out why WebKitGTK paints nothing in the launcher without GPU rendering

WebKit's GPU buffers are the largest part of Torchsnap's memory on the Linux testbed: about 190 MiB at start and 146 to 157 MiB after three minutes hidden (Raspberry Pi 5, GNOME 48 Wayland, WebKitGTK 2.54, release build; Linux memory review, 2026-10-24). Without GPU rendering, memory fell by about 120 to 150 MiB, but the launcher did not paint.

## What was seen

- `GDK_GL=disable` (whole UI process; WebKit logs "Disabled hardware acceleration because GTK failed to initialize GL"): in the transparent launcher window only fragments appear (patches of the emoji grid that changed, no card, no mascot). With an opaque launcher window it paints correctly, inside a white 808x860 box around the card. Settings and welcome windows render.
- WebKit's `hardware-acceleration-policy` set to `NEVER` on the launcher's webview through `with_webview` after the window was built: nothing paints, transparent or opaque (a black box), on every show and after any wait.
- In every case the window was mapped, visible and focused, the control API answered and the page reported its content as visible.
- `WEBKIT_DISABLE_DMABUF_RENDERER=1` takes the same CPU path and showed the same memory drop; it was not checked on screen.

## What working CPU rendering cost

With the opaque window, against an opaque GPU baseline: 118 to 151 MiB less in every phase, show to visible content 136 against 53 ms median, web-process CPU while shown 27 against 6 % of a core, GNOME Shell 13 against 9 %. Idle CPU while hidden was the same.

## Not tried

- Setting the policy before the page loads, on the `WebKitSettings` the webview is created with, which wry does not expose.
- Other compositors (labwc, KDE, X11) and other GPUs.
