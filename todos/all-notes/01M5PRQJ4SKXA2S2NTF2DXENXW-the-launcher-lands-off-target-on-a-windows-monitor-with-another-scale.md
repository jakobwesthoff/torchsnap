---
title: "The launcher lands off target on a Windows monitor with another scale factor"
kind: bug
component: platform
impact: low
origin: review
---
# The launcher lands off target on a Windows monitor with another scale factor

Found while planning the work-area placement of the launcher (2026-10-24), read in the code and the tao source, not reproduced. On Windows, when the launcher moves to a monitor whose scale factor differs from the one the window is on, its position is converted with the old monitor's factor, so it lands off the computed spot. macOS and Linux are not affected.

## Problem

`launcher_window_origin` (`src-tauri/src/monitor_placement.rs`) computes the window origin in logical units of the target monitor. On Windows and Linux the launcher uses the default `LauncherPanel::set_frame` (`src-tauri/src/platform/mod.rs`, around line 127), which calls `set_position(LogicalPosition)` and then `set_size(LogicalSize)`.

tao turns a logical position into the window's native units with the window's current scale factor:

- Windows (`tao-0.35.3/src/platform_impl/windows/window.rs:229-230`): `position.to_physical::<i32>(self.scale_factor())`. `self.scale_factor()` is the factor of the monitor the window is on before the move, so a logical origin meant for a 2.0 monitor, converted at 1.0, lands at half the physical coordinates (and the reverse).
- Linux (`tao-0.35.3/src/platform_impl/linux/window.rs:457-461`): converts to logical, which GTK uses directly, so a logical input passes unchanged. Not affected.
- macOS overrides `set_frame` (NSPanel `setFrame:display:`). Not affected.

The size has the same conversion, so the window may also get the wrong size for one show; whether Windows' per-monitor DPI handling corrects it after the move is unknown.

## Impact

Only Windows with two or more monitors at different scale factors. Windows is not a release target yet.

## Suggested fix

Convert the origin and size to physical pixels with the target monitor's scale factor in the caller and pass `PhysicalPosition` and `PhysicalSize` to `set_position` and `set_size`, at least on Windows. Test with fixtures for a move from a 1.0 to a 2.0 monitor and back; check on real hardware before closing.
