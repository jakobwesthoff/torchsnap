---
kind: bug
severity: medium
status: open
area: [src-tauri/src/platform/macos/launcher_panel.rs]
tags: [macos]
---

# `set_frame` flips y using `NSScreen::mainScreen` — wrong screen on multi-monitor setups

## Problem

`MacosLauncherPanel::set_frame` converts Tauri's top-left-origin
global coordinates to AppKit's bottom-left-origin coordinates
(`src-tauri/src/platform/macos/launcher_panel.rs:157-166`):

```rust
// macOS global coordinates use bottom-left origin relative
// to the primary screen. Flip the top-left y coordinate.
let primary_height = NSScreen::mainScreen(mtm)
    .map(|s| s.frame().size.height)
    .unwrap_or(0.0);
let flipped_y = primary_height - y - height;
```

The comment and the math require the *primary* screen: AppKit's
global coordinate space has its origin at the bottom-left of the
screen at index 0 of `NSScreen.screens` (the screen whose frame
origin is `(0,0)`). But the code calls `NSScreen::mainScreen`,
which per AppKit documentation returns the screen containing the
window with keyboard focus, not the primary screen. The variable
name `primary_height` documents the intent; the API call does not
match it.

On a single-monitor machine the two are identical, so the bug is
invisible in the common development setup. With multiple displays
of different heights, whenever the key window lives on a
non-primary display (or the app's own panel became key on one),
`mainScreen` returns that display and the flip uses the wrong
height, offsetting the launcher vertically by the height
difference between the two screens.

There is also a degenerate fallback: `unwrap_or(0.0)` yields
`flipped_y = -y - height`, placing the panel far off-screen, though
`mainScreen` returning `None` is unlikely in practice.

## Impact

Launcher panel appears at the wrong vertical position (or
off-screen) on multi-monitor systems with displays of different
resolutions, depending on which screen currently holds the key
window.

## Suggested fix

Use the first entry of `NSScreen::screens(mtm)` (the primary
screen defining the global origin) instead of `mainScreen`:

```rust
let primary_height = NSScreen::screens(mtm)
    .firstObject()
    .map(|s| s.frame().size.height)
    .unwrap_or(0.0);
```

Verify against the coordinate space the caller uses (Tauri
`LogicalPosition` globals are primary-screen-relative as well) on
a two-display arrangement where the secondary display is taller
than the primary.
