---
title: "Type-check and format packages/gadget-sdk in the quality gates"
kind: chore
component: gadget-sdk
---
# Type-check and format packages/gadget-sdk in the quality gates

The frontend gadget SDK package in `packages/gadget-sdk` is outside every quality gate that `just check` and `just fullcycle` run. Its shims mirror host types by hand, so type drift there goes unnoticed until a gadget happens to import the affected subpath.

## Problem

The frontend gadget SDK package is not covered by any quality gate that `just check` / `just fullcycle` run:

- **Type checking:** the root `tsconfig.json` has `"include": ["src"]` (`tsconfig.json:37`), so `bun run tsc --noEmit` (`just check-types`, `just/quality.just:37-38`) never looks at `packages/gadget-sdk/src/**`. The package has no `tsconfig.json` of its own (`ls packages/gadget-sdk/` shows `package.json`, `src`, `theme.css`), so nothing type-checks it standalone.
- **Formatting:** the prettier globs are `'src/**/*.{ts,tsx,css}' '*.{js,ts,json}'` (`package.json` scripts `fmt` / `fmt:check`). `packages/` is not matched.
- **Linting:** `eslint .` does cover it (flat config matches `**/*.{ts,tsx}` with only `dist`/`src-tauri` ignored), but with the non-type-aware ruleset only.

The package is exactly the code where type errors are costly: the shims declare the `window.__torchsnap` contract via global interface merging (`packages/gadget-sdk/src/shims/*.ts`) and mirror host types by hand ("The hook *types* are mirrored locally", `shims/hooks.ts`). Drift between a shim's mirrored type and the host implementation is only caught indirectly, when some gadget frontend that happens to import the affected subpath runs its own `tsc` during `just build-gadget`. Files nothing imports yet (e.g. `src/testing/MockGadgetContextProvider.tsx`) are checked by nobody.

## Suggested fix

- Add `packages/gadget-sdk/tsconfig.json` (strict, jsx, bundler resolution, mirroring the root options) and wire it into `check-types` (`tsc --noEmit -p packages/gadget-sdk` or a project reference from the root config).
- Extend the prettier globs to `'packages/**/*.{ts,tsx,css}'`.
- The type-sync check mentioned in `src/lib/sdk.ts` (GadgetContext importing `shims/hooks.ts`) partially guards the hooks slice. The components and keybindings slices have no equivalent. A compile-time assignment check in the new package tsconfig scope would close that.
