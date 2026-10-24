# 72. Place the launcher card in the monitor's work area

Date: 2026-10-24

## Status

Accepted

Amends [20. Size launcher webview to content and shrink on hide](0020-size-launcher-webview-to-content-and-shrink-on-hide.md)

Amends [71. Reserve no shadow pad above the launcher card](0071-reserve-no-shadow-pad-above-the-launcher-card.md)

## Context

ADR 20 centres the launcher window on the monitor under the cursor and
positions it so that the card lies at a quarter of the screen height.
ADR 71 states the same rule for macOS and Windows: the card's top lies
at a quarter of the monitor's height. Both measure the whole monitor,
including the strips that the menu bar, the Dock, the task bar and
panels cover.

`tauri::Monitor::work_area()` returns the part of a monitor that these
leave free. Tauri 2.11.6 reads it from `NSScreen.visibleFrame` on
macOS, `rcWork` on Windows and `gdk_monitor_get_workarea` on Linux, and
returns the whole monitor where a platform reports no work area. On
Wayland GDK has no work area, so the value is the whole monitor, and
the compositor places the window.

## Decision

The launcher window is centred horizontally on the monitor's work area,
and the card's top lies at a quarter of the work area's height below
the work area's top. The window's top is never above the work area's
top. Nothing clamps the window's bottom.

The work area is divided by the monitor's scale factor, as the monitor
rect is. On macOS that gives points for the work area of each monitor,
whatever its scale factor.

Only the launcher uses the work area. The other windows stay centred
on the whole monitor, and `targetMonitor` in the control API keeps
reporting the whole monitor rect.

## Consequences

- With a work area equal to the monitor, the origin is the one ADR 20
  and 71 describe.
- With a menu bar or top bar of height `b` on a monitor of height `h`,
  the card's top lies at `b + (h - b) / 4`, not `h / 4`.
- The top clamp acts only when a quarter of the work area's height is
  smaller than the card's offset below the window's top, which is
  181.9 px (ADR 71). With a 35 px bar that is a work area under about
  590 px high.
- On Wayland nothing changes, as the compositor places the window.
- On X11 desktops other than GNOME, GDK applies the work area to the
  primary monitor only, so other monitors keep the whole monitor.
