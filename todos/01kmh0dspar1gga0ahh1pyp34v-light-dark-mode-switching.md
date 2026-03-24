# Light/dark mode switching

The ThemeProvider infrastructure is in place but there is no UI to
switch between light, dark, and system themes.

## What needs to happen

1. Add a theme toggle as a built-in launcher command (see default
   commands todo)
2. Add a theme toggle section to the settings panel (see settings
   toggle todo)
3. The `useTheme` hook already provides `setPreference()` — just
   needs UI wiring
