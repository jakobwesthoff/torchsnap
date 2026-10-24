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

## Cause (found in the WebKit 2.54.0 source, not yet reproduced on the Pi)

Without GPU rendering WebKit uses its non-composited frame renderer. Each frame it clears the whole render target to transparent when the page background is not opaque (`NonCompositedFrameRenderer.cpp:211`, `AcceleratedSurface.cpp:1106-1124`) and redraws only the damaged rectangles (`:241-249`). wry sets a (0,0,0,0) webview background for a transparent window, so only the changed patches survive each frame. An opaque background skips the clear. The code is unchanged on WebKit `main`. Other WebKitGTK 2.54 apps report the same and switched to an opaque window (https://github.com/aula-id/koma/issues/242, https://github.com/axpdev-lab/aeroftp/pull/1240, https://github.com/NC1107/sink/pull/79, https://github.com/SimoHypers/limusic/issues/523); none cites a WebKit bug.

`hardware-acceleration-policy` is fixed when the page is created: the renderer is chosen at drawing-area creation (`DrawingAreaCoordinatedGraphicsGLib.cpp:151-157`) and the surface keeps its GPU flag from construction (`AcceleratedSurface.cpp:122`). Setting the policy after load leaves a GPU renderer under CPU preferences. wry and Tauri do not expose settings at creation, and setting it at creation would still end in the non-composited renderer.

`WEBKIT_DISABLE_COMPOSITING_MODE=1`, measured as worse for memory in the same review, takes the same renderer and was not checked on screen.

## Open experiments

- Separate the two causes: an opaque GTK window with the webview background left at (0,0,0,0) and `GDK_GL=disable`. Fragments mean the web-process clear is the cause.
- A page-side workaround: while shown, a viewport-sized element that changes every frame, so every frame's damage is the whole view. If the picture is complete, measure the web process's CPU and the time to visible content.
- The policy set at creation through a temporary wry patch: expect the same picture as `GDK_GL=disable`.
- `about:gpu` in a throwaway view under each switch, to see which renderer each one really takes.

Details and sources: the WebKitGTK rendering research of 2026-10-24.

## Not tried

Other compositors and GPUs do not change the cause according to the source (no display-server branch), so they are low value. Fixing `NonCompositedFrameRenderer::updateRendering` upstream (clear only the damage region) would help only once distributions ship it.
