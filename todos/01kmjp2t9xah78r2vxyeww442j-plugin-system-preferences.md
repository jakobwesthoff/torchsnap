# Plugin: System Preferences / Settings Panes

Catalog plugin that indexes system settings panes (macOS System
Settings, Linux system panels, Windows Settings pages) and opens them
directly from the launcher.

## Scope

- Index available system settings panes with display names
- Fuzzy match against search query
- Open the selected pane directly

## Platform considerations

- **macOS**: System Settings panes accessible via URL scheme
  `x-apple.systempreferences:<pane-id>` or via
  `mdfind "kMDItemContentType == 'com.apple.systempreference.prefpane'"`.
  Modern macOS (Ventura+) uses the Settings app with different pane
  identifiers.
- **Linux**: Depends on desktop environment (GNOME Settings, KDE
  System Settings). Could parse `.desktop` files with
  `Categories=Settings`.
- **Windows**: `ms-settings:` URI scheme for Windows 10+ Settings app.

## Implementation notes

- This is a catalog plugin (finite list, host filters via nucleo)
- Separate from the app launcher plugin to keep concerns clean
- Icons: each pane has its own icon (extract or use generic gear)
- Consider caching the pane list since it rarely changes
