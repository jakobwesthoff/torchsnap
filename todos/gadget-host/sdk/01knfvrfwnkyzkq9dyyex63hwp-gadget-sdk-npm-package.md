# Gadget SDK npm Package (`@torchsnap/gadget-sdk`)

**Status:** partially complete. The SDK now lives as a proper
workspace package at `packages/gadget-sdk/` with a real
`package.json` (exports map, Vite gadget, hooks/components/testing
subpaths). In-repo consumers — gadgets and the host — depend on it
via `"@torchsnap/gadget-sdk": "file:../../../packages/gadget-sdk"`
(or workspace-root `file:./packages/gadget-sdk`), so the "relative
path aliases into the monorepo" problem is solved for in-tree
development. What remains is the *external* publishing story
(actually shipping the package to the npm registry with
generated `.d.ts` artifacts); the notes below document that
deferred work.

Publish the `packages/gadget-sdk/` workspace package to npm so
third-party gadget authors can `bun add -D @torchsnap/gadget-sdk`
instead of cloning the monorepo.

## Scope

- **Type definitions** (`.d.ts`): `GadgetViewProps`, `InlineViewProps`,
  `GadgetSettingsProps`, `ActionId`, `SourcedEntry`, `FooterState`, etc.
  Generated/copied from host source at publish time — single source of
  truth stays in `src/`.
- **Runtime shims**: `react`, `react/jsx-runtime`, `@torchsnap/components`,
  `@torchsnap/keybindings`, `@torchsnap/hooks` — re-export from
  `window.__torchsnap.*`.
- **Vite gadget** (`@torchsnap/gadget-sdk/vite`): auto-configures React
  shim aliases, JSX runtime, and lib-mode output settings. Gadget authors
  just add `torchsnapGadget()` to their Vite config.
- **Theme CSS**: shared Tailwind v4 design tokens for gadget builds.

## Design Decisions

### Type duplication avoidance

During in-repo development, gadget `tsconfig.json` uses `paths` to
point `@torchsnap/types` at the host's `src/types.ts` via relative
paths. Since gadgets only use `import type`, these are erased at compile
time — no runtime shim needed, no duplication. The npm package replaces
this with proper `.d.ts` files copied/generated from the host source at
publish time.

### React shim architecture

React is a runtime dependency provided by the host via
`window.__torchsnap.React`. Gadgets must NOT bundle their own copy.

During in-repo development, a shim file
(`packages/gadget-sdk/src/shims/react.ts`) re-exports from the global,
and Vite `resolve.alias` redirects `import React from "react"` to the
shim. The shim gets inlined into the bundle (~2 lines), producing a
self-contained ES module with no unresolved imports.

In the npm package, this becomes a Vite gadget:

```ts
// @torchsnap/gadget-sdk/vite
export function torchsnapGadget(): VitePlugin {
  return {
    name: "torchsnap-gadget",
    config() {
      return {
        resolve: {
          alias: {
            "react": "@torchsnap/gadget-sdk/shims/react",
            "react/jsx-runtime": "@torchsnap/gadget-sdk/shims/jsx-runtime",
          },
        },
      };
    },
  };
}
```

Gadget authors just add `torchsnapGadget()` to their Vite config — the
alias machinery becomes invisible. This follows the same pattern as
VS Code extensions (where `require("vscode")` is intercepted by the
extension host) and Figma gadgets (`figma` global).

### Host SDK global expansion

`window.__torchsnap` currently only exposes `{ React }` (in
`src/lib/sdk.ts`). It needs to be expanded to include:

- `components`: `Switch`, `Slider`, `Icon`, `ShortcutRecorder`, etc.
- `keybindings`: `useKeyBindings`, `LAYER`, etc.
- `hooks`: `useGadgetSetting`, `useGadgetStream`, etc.

Each gets a corresponding shim file in `gadget-sdk/shims/` and
eventually a package entry point in the npm package.

### Once aliases are no longer needed

With a proper npm package, the `resolve.alias` entries in each gadget's
`vite.config.ts` disappear. The Vite gadget in the SDK handles React
shimming. Type imports resolve from `node_modules` normally. The only
remaining special handling is React (always host-provided, never
bundled) — the Vite gadget takes care of that permanently.

## When

After the `gadget-sdk/` directory stabilizes through 1-2 real gadget
migrations. The in-repo alias approach works fine during development —
the npm package is for third-party consumption.

## References

- Current shim approach: `gadget-sdk/shims/` (to be created)
- Host SDK global: `src/lib/sdk.ts`
- Type sources: `src/types.ts`, `src/gadgets/types.ts`
- Related: the Rust-side gadget SDK crate, already shipped at
  `gadgets/gadget-sdk/`
