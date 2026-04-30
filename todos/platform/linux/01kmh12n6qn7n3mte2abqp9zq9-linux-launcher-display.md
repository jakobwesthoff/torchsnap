# Linux launcher display support

The current launcher display works on macOS via NSPanel. The
fallback implementation uses a regular Tauri window which compiles
on Linux but needs testing and likely adjustments to feel right.

## What needs to happen

- Test the launcher on Linux (X11 and Wayland)
- Verify transparent window works (Tauri + transparent webview
  on Linux requires compositor support)
- Check window positioning on multi-monitor setups
- Investigate Linux-native overlay approaches:
  - **Wayland**: layer-shell protocol (`zwlr_layer_shell_v1`) for
    overlay windows that don't steal focus
  - **X11**: `_NET_WM_WINDOW_TYPE_DOCK` or override-redirect
    windows, `_NET_WM_STATE_ABOVE` for always-on-top
- Tray icon behavior varies by DE (GNOME removed tray, KDE has
  it, etc.) — may need `libappindicator` or `StatusNotifierItem`
- Global shortcut registration may need `XGrabKey` on X11 or
  portal-based shortcuts on Wayland
- No `ActivationPolicy::Accessory` equivalent — the app will
  appear in the taskbar. Consider minimizing to tray on close.

## Testing targets

- GNOME on Wayland (Ubuntu default)
- KDE Plasma on Wayland
- X11 fallback (any DE)
