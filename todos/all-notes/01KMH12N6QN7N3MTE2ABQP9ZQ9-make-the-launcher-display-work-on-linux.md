---
title: "Make the launcher display work on Linux"
kind: improvement
component: platform
---

# Make the launcher display work on Linux

The launcher display works on macOS through an NSPanel. The fallback
implementation uses a regular Tauri window. It compiles on Linux, but it
needs testing and probably adjustments before it feels right.

## Proposal

- Test the launcher on Linux (X11 and Wayland).
- Verify transparent window works (Tauri + transparent webview on Linux
  requires compositor support).
- Check window positioning on multi-monitor setups.
- Investigate Linux-native overlay approaches:
  - Wayland: layer-shell protocol (`zwlr_layer_shell_v1`) for overlay
    windows that don't steal focus.
  - X11: `_NET_WM_WINDOW_TYPE_DOCK` or override-redirect windows,
    `_NET_WM_STATE_ABOVE` for always-on-top.
- Tray icon behavior varies by DE (GNOME removed tray, KDE has it, etc.).
  It may need `libappindicator` or `StatusNotifierItem`. Raspberry Pi
  OS's panel `wf-panel-pi` loads a `tray` widget in its default config
  (read 2026-10-23, not tried with Torchsnap).
- Global shortcut registration may need `XGrabKey` on X11 or
  portal-based shortcuts on Wayland.
- No `ActivationPolicy::Accessory` equivalent, so the app appears in the
  taskbar. Consider minimizing to tray on close.

## Multi-monitor placement

Read in the tao 0.35.3 source on 2026-10-20, not tested on Linux. The
monitor choice in `src-tauri/src/monitor_placement.rs` compares the
cursor and the monitor rects as reported (`CoordinateSpace::Shared`)
everywhere but macOS, and on Linux those values are not in one space
when monitors have different scale factors:

- `linux/monitor.rs:29-46`: a monitor's `position()` and `size()` are
  its GDK geometry times that monitor's own scale factor.
- `linux/util.rs:18-33`: on X11 the cursor position is the pointer's
  logical position times the default display group's scale factor. On
  Wayland it is always (0, 0), so the choice is whichever monitor
  contains the origin.

This is the same kind of bug that `CoordinateSpace::MacosPoints` fixes
on macOS. A Linux fix needs its own coordinate space, and the window
centering in `centered_window_origin` needs checking with it: tao on
Linux converts the requested position to logical coordinates with the
window's own scale factor (`linux/window.rs:457-461`).

## Testing targets

- GNOME on Wayland (Ubuntu default). Mutter has no layer-shell, so the
  overlay approach above does not apply there.
- labwc on Wayland (Raspberry Pi OS default). A wlroots compositor with
  layer-shell.
- KDE Plasma on Wayland
- X11 fallback (any DE)
