---
title: "Make the launcher display work on Windows"
kind: improvement
component: platform
---

# Make the launcher display work on Windows

The launcher display works on macOS through an NSPanel. The fallback
implementation uses a regular Tauri window. It compiles on Windows, but it
needs testing and adjustments.

## Proposal

- Test the launcher on Windows 10 and 11.
- Verify transparent window works (Tauri + WinAPI transparency).
- Check window positioning on multi-monitor setups. DPI scaling varies per
  monitor on Windows.
- Investigate a Windows-native non-activating window:
  - `WS_EX_NOACTIVATE` extended window style, which prevents the window from
    becoming the foreground window when clicked.
  - `WS_EX_TOOLWINDOW`, which hides the window from the taskbar and Alt+Tab.
  - May need raw Win32 API calls similar to the NSPanel approach.
- System tray: Windows has native tray icon support via Tauri's built-in
  tray. It should work out of the box.
- Global shortcut: `RegisterHotKey` API via `tauri-plugin-global-shortcut`.
  It should work, but test with common conflicts (Win+Space is already taken
  by the input method).
- Consider `WS_EX_TOPMOST` for always-on-top behavior.
- Test with Windows high-contrast themes.

## Testing targets

- Windows 11 (primary)
- Windows 10 (secondary)
- Multi-monitor with mixed DPI scaling
