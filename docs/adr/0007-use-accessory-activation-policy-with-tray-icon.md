# 7. Use Accessory activation policy with tray icon

Date: 2026-03-24

## Status

Accepted

## Context

A launcher app should be invisible in the dock and app switcher. It
should be reachable only via its global shortcut and a menubar tray
icon. On macOS, `ActivationPolicy::Accessory` removes the app from
both the dock and Cmd+Tab. On Linux and Windows, there is no exact
equivalent — the app will appear in the taskbar.

## Decision

Set `ActivationPolicy::Accessory` on macOS in the `setup()` callback
before the run loop starts. Provide a tray icon with a context menu
containing "Settings..." and "Quit Torchsnap" entries. Left-clicking
the tray icon toggles the launcher; right-clicking opens the context
menu.

The global shortcut (default `CmdOrCtrl+Shift+Space`) is the primary
activation mechanism. The shortcut is persisted via
`tauri-plugin-store` and can be updated at runtime through the
`update_global_shortcut` IPC command.

## Consequences

- No dock icon or Cmd+Tab entry on macOS — the tray icon and
  shortcut are the only ways to interact with the app.
- On Linux/Windows, the app will have a taskbar presence until
  platform-specific equivalents are implemented.
- The tray icon currently uses the generic app icon (32x32.png); a
  dedicated monochrome template icon should be created for proper
  macOS menubar styling.
- `macOSPrivateApi: true` is required in `tauri.conf.json` to enable
  transparent windows and the private activation policy API.
