# Extract shared component library for plugin use

All hooks, utilities, and base components should live in a separate
package so that future plugins can import and use them.

## Reference

Squirly uses `@squirly/acornkit` as a shared library containing:
- Theme system (ThemeProvider, ThemeContext, useTheme)
- Hooks (useEmacsBindings, useKeyboardNavigation, useSearch, etc.)
- Launcher components (SearchInput, LauncherItemRow, LauncherFooter)
- Utilities (cn)
- Design tokens (tokens.css)

## What needs to happen

1. Decide on package structure:
   - Separate directory (e.g. `packages/torchsnap-kit/`) with its
     own `package.json`?
   - Or a `src/kit/` directory with barrel exports?
2. Move shared code into the library:
   - `cn` utility
   - Theme system (ThemeProvider, ThemeContext, useTheme)
   - Settings store (settingsStore, useSetting, settingsDefaults)
   - Window lifecycle hook
   - Design tokens CSS
3. Update imports in launcher and settings entry points
4. Ensure the library is consumable by WASM-based plugins (see
   plugin system todo)

## Considerations

- The library should have a stable public API since plugins will
  depend on it
- TypeScript type exports are important for plugin DX
- Keep the library tree-shakeable
