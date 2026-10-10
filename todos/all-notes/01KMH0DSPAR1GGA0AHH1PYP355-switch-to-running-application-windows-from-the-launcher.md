---
title: "Switch to running application windows from the launcher"
kind: feature
component: gadgets
status: needs-discussion
horizon: someday
---

# Switch to running application windows from the launcher

The launcher should list running applications and windows and switch to
them. Listing the windows needs a platform accessibility or window API
on each OS, and the open questions below need a decision before the
work starts.

## Scope

- List all running applications with their window titles
- Fuzzy search by app name or window title
- Select to bring that window to the foreground
- Show app icons

## Platform considerations

- **macOS**: `NSWorkspace.runningApplications` + Accessibility API
  for window titles (requires accessibility permission)
- **Linux**: `wmctrl`, `xdotool`, or direct X11/Wayland protocol
- **Windows**: `EnumWindows` API

## Challenges

- Accessibility permission prompt on macOS
- Multiple windows per app: list individually or grouped?
- Some apps have many windows (browser tabs?). Probably don't
  list individual tabs.
- Performance: refreshing the window list on every keystroke vs
  caching with a refresh interval

## Revisit when

No trigger known; reconsider at the next sweep.
