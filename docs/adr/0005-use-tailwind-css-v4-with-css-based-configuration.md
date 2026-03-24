# 5. Use Tailwind CSS v4 with CSS-based configuration

Date: 2026-03-24

## Status

Accepted

## Context

The UI needs a consistent design system with dark mode support across
two independent webview windows. The styling approach must support
semantic theming (surfaces, text levels, borders, accents) without
scattering `dark:` variant classes throughout every component.

## Decision

Use Tailwind CSS v4 with the new CSS-based configuration (no
`tailwind.config.js`). Theme tokens are defined as CSS custom
properties in `src/index.css` using `@theme` and overridden under
`[data-theme="dark"]` selectors. The dark variant is declared as:

```css
@custom-variant dark (&:where([data-theme="dark"], [data-theme="dark"] *));
```

Components reference semantic tokens (`bg-surface`,
`text-text-primary`, `border-border`) that automatically resolve to
the correct light/dark values based on the `data-theme` attribute
set by the `ThemeProvider`.

The `ThemeProvider` context manages preference persistence
(localStorage), system theme detection (`prefers-color-scheme`),
and cross-window sync via the browser's `StorageEvent`.

## Consequences

- Components use semantic class names without `dark:` prefixes.
- Adding a new theme (e.g. high-contrast) means adding another
  `[data-theme="high-contrast"]` block in `index.css`.
- The `cn()` utility (clsx + tailwind-merge) is used for dynamic
  class composition with proper conflict resolution.
- Theme changes propagate across all open windows automatically.
