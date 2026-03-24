# 4. Use multi-page Vite build for launcher and settings

Date: 2026-03-24

## Status

Accepted

## Context

The launcher and settings windows have fundamentally different HTML
requirements. The launcher needs a transparent background
(`background: transparent`) and no overflow scrolling. The settings
window needs a normal opaque background with dark-mode flash
prevention via inline `<style>` blocks.

Both windows run as separate Tauri webviews with independent
JavaScript runtimes — they cannot share a single-page app with
client-side routing.

## Decision

Use Vite's multi-page build via `build.rollupOptions.input` to
compile two separate entry points:

- `launcher.html` → `src/launcher/main.tsx`
- `settings.html` → `src/settings/main.tsx`

Each page has its own React root, CSS imports, and inline styles
tailored to its window type. Shared code (settings store, hooks,
theme provider, utilities) lives in `src/` and is imported by both
entry points — Vite's tree-shaking ensures each bundle only includes
what it uses.

## Consequences

- Each window loads only the code it needs.
- HTML-level concerns (background color, overflow, user-select) are
  handled per-page without runtime conditionals.
- Adding a new window type means adding a new HTML file and entry
  point to the Vite config.
- During development, the Tauri dev server serves both pages from
  the same Vite dev server on port 1420.
