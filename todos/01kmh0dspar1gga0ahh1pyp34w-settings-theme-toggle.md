# Add light/dark/system toggle to settings panel

## Reference

Nutty's `AppearanceSection` in the settings panel provides a
three-way toggle: Light, Dark, System. It uses the `useTheme` hook
from acornkit to read and write the preference.

## What needs to happen

1. Create an `AppearanceSection` component in `src/settings/`
2. Three-way segmented control or button group: Light / Dark / System
3. Wire to `useTheme().setPreference()`
4. The ThemeProvider already handles localStorage persistence and
   cross-window sync — the UI just needs to call `setPreference`
