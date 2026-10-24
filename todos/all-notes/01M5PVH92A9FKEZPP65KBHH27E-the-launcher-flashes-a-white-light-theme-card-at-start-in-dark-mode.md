---
title: "The launcher flashes a white light-theme card at start in dark mode"
kind: bug
component: frontend
origin: incident
tags: [linux, ux]
---
# The launcher flashes a white light-theme card at start in dark mode

When Torchsnap starts on the Linux testbed (GNOME in dark mode), the
launcher briefly shows a white card in the light theme with an empty
results area, then the dark launcher with its mascot. Jakob saw it on
the Pi on 2026-10-24 as a white rectangle in the middle of the screen.

## Observation

Recorded on 2026-10-24 on the Pi with GNOME's screencast (frames only
on change, 110 to 250 ms apart near the start), app code of
linux-testbed 2ccc727c, four cold starts:

- The white card (search field, empty white area below it, no mascot,
  about 680x352 logical pixels in the centre of a 1080x1920 portrait
  screen) shows in exactly one frame in 3 of 4 starts: release at
  3.775 s and 3.752 s, debug at 4.038 s. The next frames, 156 to 191 ms
  later, show the dark launcher. The third release start caught no
  white frame.
- It appears about 1 s after the app's control socket answers.

Not checked on macOS.

## Likely cause (from the code, not verified)

The light theme is the CSS default: `packages/gadget-sdk/theme.css`
switches to dark only under `[data-theme="dark"]` on `<html>`.
`src/contexts/ThemeProvider.tsx` sets that attribute in a `useEffect`,
which runs after React's first commit, so a frame painted before it is
light. The window itself is on screen that early because on Linux the
launcher window is built visible: WebKitGTK does not run the webview
while its window is unmapped, so the window stays mapped until the
first dismiss (`src-tauri/src/lib.rs`, `.visible(false)` only off
Linux). The empty results area fits a window shown before the frontend
has rendered its content.

## Decisions

- 2026-10-24, Jakob: at this point the window should not be visible at
  all, so the goal is that nothing of the launcher shows before its
  content is rendered. If Linux does not allow that, the default before
  the theme applies should be transparent rather than light, so an
  early frame shows nothing.

## Suggested direction

First try to keep the window from showing before the content: starting
the launcher hidden on Linux is the subject of the related
investigation below. If that fails, make the early frames transparent:
set `data-theme` before the first paint (an inline script in the HTML
that reads the stored preference and `prefers-color-scheme`) and keep
the card from painting until the frontend has rendered it. Then record
cold starts on the testbed again; the frame interval of the recorder
(above) limits what a recording can rule out.

## Relations

- Relates to: [Decide whether the Linux launcher can realize its webview while hidden](01KPV8GR68K0P9J90GNTV766DR-decide-whether-the-linux-launcher-can-realize-its-webview-while-hidden.md)
