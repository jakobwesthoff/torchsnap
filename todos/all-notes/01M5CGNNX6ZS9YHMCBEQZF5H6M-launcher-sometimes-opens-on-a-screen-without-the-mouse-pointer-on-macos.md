---
title: "Launcher sometimes opens on a screen without the mouse pointer on macOS"
kind: bug
component: platform
origin: incident
---
# Launcher sometimes opens on a screen without the mouse pointer on macOS

Observed on 2026-10-20 on a second macOS machine with more than one
display, not on the development machine: the launcher sometimes opens
on a different screen from the one the mouse pointer is on, although
the code means to place it on the pointer's screen. It happens
intermittently, and the conditions that trigger it are not known yet.
One suspicion is that it starts once the launcher was opened by a left
click on the menu bar icon, but that is not confirmed.

## Problem

Seen in use, not reproduced. The machine runs a macOS older than 27,
so a left click on the menu bar icon toggled the launcher directly.
Unknown so far: the exact macOS version and display arrangement,
whether the displays use different scale factors or sizes, and whether
the wrong placement follows the global hotkey, the menu bar icon or
both.

Where the screen is chosen: `position_launcher_on_cursor_monitor` in
`src-tauri/src/lib.rs` calls `monitor_under_cursor`, which picks the
monitor whose rect contains `app.cursor_position()` and falls back to
the primary monitor when none matches. `MacosLauncherPanel::set_frame`
in `src-tauri/src/platform/macos/launcher_panel.rs` then converts the
position to AppKit coordinates.

Leads to check, none verified:

- `monitor_under_cursor` compares the cursor against the monitors'
  physical `position()` and `size()`. With displays of different scale
  factors, physical rects of neighbouring monitors may overlap or leave
  gaps, so the cursor can match the wrong monitor or none and fall back
  to the primary one. Check that `cursor_position()` and the monitor
  rects are in the same coordinate space.
- The y flip in `set_frame` takes its height from
  `NSScreen::mainScreen`, the screen holding the key window, so the
  result depends on which window is key (see the related todo). An
  opening through the menu bar icon may change which window is key and
  so the height later openings flip against. On displays of different
  heights, the vertical offset could push the launcher onto a
  neighbouring display. This would fit the menu bar suspicion.

## Impact

The user presses the hotkey or clicks the icon and the launcher appears
on a screen they are not looking at.

## Suggested fix

Reproduce first. Get the display arrangement, scale factors and macOS
version of the affected machine, log the cursor position, the monitor
list, the chosen monitor and the frame passed to `set_frame`, and
compare hotkey and tray openings, before and after a tray opening. Then
fix the lead that holds, with a regression test for the monitor choice
where it can be tested without real displays.

## Relations

- Relates to: [Launcher panel y flip uses the wrong screen on macOS multi-monitor setups](01KWG2AWFPCAQ7PSH6G90GRJPV-launcher-panel-y-flip-uses-the-wrong-screen-on-macos-multi-monitor.md), same placement path
