# Gadget: Task switcher

List running applications/windows and switch to them from the
launcher.

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
- Multiple windows per app — list individually or grouped?
- Some apps have many windows (browser tabs?) — probably don't
  list individual tabs
- Performance: refreshing the window list on every keystroke vs
  caching with a refresh interval
