# 74. Shrink the Linux launcher while it is mapped before hiding it

Date: 2026-10-24

## Status

Accepted

Amends [20. Size launcher webview to content and shrink on hide](0020-size-launcher-webview-to-content-and-shrink-on-hide.md)

## Context

ADR 0020 hides the launcher and then shrinks its window to 1x1. On
Linux (Raspberry Pi 5, GNOME 48 on Wayland, WebKitGTK 2.54) the shrink
after the hide frees nothing: the hidden window's GPU buffers stay the
same with and without it.

Shrinking to 1x1 while the window is still mapped frees the window
buffers shared between the UI process, the web process and GNOME Shell
(the UI process's shared GPU memory falls from 40.9 to 8.8 MiB, the web
process's from 21.4 MiB to 0). The web process's own GPU memory stays.
With a 150 ms wait before the hide, Torchsnap's memory 60 s after a
dismiss was 29.8 MiB below that of today's hide.

The buffers are released after the shrink, not with it. The web process
released a median 12.7 ms after the shrink and the UI process about
28 ms after it; over 200 dismisses the latest release was 121.7 ms
after the shrink. With a fixed wait between the shrink and the hide, 0
and 16 ms passed the release test in 0 of 20 dismisses, 50 ms in 19 and
150 ms in 20. With the 150 ms wait built in, 200 of 200 dismisses
passed it. The median time from dismiss to hidden was 42 ms with
today's hide and 204 ms with a 150 ms wait.

On Linux the frontend blanks `#root` for two frames before it asks for
the hide (`src/launcher/visibility.ts`), so the page is blank while the
window shrinks.

## Decision

On Linux, `hide_launcher` shrinks the window to 1x1 while it is mapped
and hides it 150 ms later (`MAPPED_SHRINK_WAIT` in
`src-tauri/src/launcher_hide.rs`). The wait is fixed, not tied to a
signal from GTK or WebKit. The call returns at once; the hide runs on
the main thread when the wait is over. The launcher window keeps its
fixed size on Linux and is not maximized.

All Linux hide paths take this sequence: the frontend's `launcher_hide`,
the fallback in `request_launcher_dismiss` and the control API's `hide`.

A launcher waiting for its hide counts as hidden. A show, a toggle or a
control API `show` in that time cancels the hide and shows the launcher
the normal way, with the frame set again at full size. A second dismiss
in that time starts no second timer. A show that cancels a hide emits
`launcher-hide-cancelled`, and the launcher draws its next mascot on it
as it does on a blur, since no blur comes when the hide never happens.

macOS and Windows keep ADR 0020's order: hide, then shrink.

## Consequences

- On Linux the control API's `status` reports the launcher as visible
  until the hide, 150 ms after the shrink.
- On Linux the dismiss takes about 160 ms longer to reach the hidden
  state.
