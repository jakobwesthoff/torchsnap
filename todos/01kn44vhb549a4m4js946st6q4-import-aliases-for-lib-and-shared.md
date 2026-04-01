# Set up import aliases for lib and shared frontend modules

## Problem

The frontend currently uses relative paths everywhere (e.g.,
`../../lib/pluginMessage`, `../hooks/useSetting`). This makes it
harder to:

- Move files between directories without cascading import updates.
- Identify at a glance whether an import is project-local or from
  `node_modules`.
- Decouple shared logic for reuse in the upcoming WASM/plugin
  frontend, where plugin UIs need access to a subset of the
  launcher's lib/hooks/components without pulling in Tauri-specific
  code.

## What's needed

1. Audit the current `tsconfig.json` / Vite config for existing path
   aliases (if any).
2. Define aliases for key directories — candidates:
   - `@lib/` → `src/lib/`
   - `@hooks/` → shared hooks (currently split across
     `src/launcher/hooks/` and `src/settings/hooks/` — may need
     consolidation first)
   - `@components/` → shared components
   - `@types/` or `@shared/` → shared type definitions
3. Configure both TypeScript (`paths` in `tsconfig.json`) and Vite
   (`resolve.alias`) so aliases resolve correctly at build time and
   in the IDE.
4. Refactor existing imports to use aliases.

## Context

This is preparatory work for the WASM/plugin decoupling. When plugin
frontends run in their own context they'll need to import shared
utilities (e.g., `sendPluginMessage`, UI components, types) via clean
aliases rather than fragile relative paths that assume the launcher's
directory structure. Getting aliases in place now avoids a larger
migration later.
