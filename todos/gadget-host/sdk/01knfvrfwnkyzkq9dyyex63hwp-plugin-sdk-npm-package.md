# Plugin SDK npm Package (`@torchsnap/plugin-sdk`)

**Status:** partially complete. The SDK now lives as a proper
workspace package at `packages/plugin-sdk/` with a real
`package.json` (exports map, Vite plugin, hooks/components/testing
subpaths). In-repo consumers — plugins and the host — depend on it
via `"@torchsnap/plugin-sdk": "file:../../../packages/plugin-sdk"`
(or workspace-root `file:./packages/plugin-sdk`), so the "relative
path aliases into the monorepo" problem is solved for in-tree
development. What remains is the *external* publishing story
(actually shipping the package to the npm registry with
generated `.d.ts` artifacts); the notes below document that
deferred work.

Publish the `packages/plugin-sdk/` workspace package to npm so
third-party plugin authors can `bun add -D @torchsnap/plugin-sdk`
instead of cloning the monorepo.

## Scope

- **Type definitions** (`.d.ts`): `PluginViewProps`, `InlineViewProps`,
  `PluginSettingsProps`, `ActionId`, `SourcedEntry`, `FooterState`, etc.
  Generated/copied from host source at publish time — single source of
  truth stays in `src/`.
- **Runtime shims**: `react`, `react/jsx-runtime`, `@torchsnap/components`,
  `@torchsnap/keybindings`, `@torchsnap/hooks` — re-export from
  `window.__torchsnap.*`.
- **Vite plugin** (`@torchsnap/plugin-sdk/vite`): auto-configures React
  shim aliases, JSX runtime, and lib-mode output settings. Plugin authors
  just add `torchsnapPlugin()` to their Vite config.
- **Theme CSS**: shared Tailwind v4 design tokens for plugin builds.

## Design Decisions

### Type duplication avoidance

During in-repo development, plugin `tsconfig.json` uses `paths` to
point `@torchsnap/types` at the host's `src/types.ts` via relative
paths. Since plugins only use `import type`, these are erased at compile
time — no runtime shim needed, no duplication. The npm package replaces
this with proper `.d.ts` files copied/generated from the host source at
publish time.

### React shim architecture

React is a runtime dependency provided by the host via
`window.__torchsnap.React`. Plugins must NOT bundle their own copy.

During in-repo development, a shim file
(`packages/plugin-sdk/src/shims/react.ts`) re-exports from the global,
and Vite `resolve.alias` redirects `import React from "react"` to the
shim. The shim gets inlined into the bundle (~2 lines), producing a
self-contained ES module with no unresolved imports.

In the npm package, this becomes a Vite plugin:

```ts
// @torchsnap/plugin-sdk/vite
export function torchsnapPlugin(): VitePlugin {
  return {
    name: "torchsnap-plugin",
    config() {
      return {
        resolve: {
          alias: {
            "react": "@torchsnap/plugin-sdk/shims/react",
            "react/jsx-runtime": "@torchsnap/plugin-sdk/shims/jsx-runtime",
          },
        },
      };
    },
  };
}
```

Plugin authors just add `torchsnapPlugin()` to their Vite config — the
alias machinery becomes invisible. This follows the same pattern as
VS Code extensions (where `require("vscode")` is intercepted by the
extension host) and Figma plugins (`figma` global).

### Host SDK global expansion

`window.__torchsnap` currently only exposes `{ React }` (in
`src/lib/sdk.ts`). It needs to be expanded to include:

- `components`: `Switch`, `Slider`, `Icon`, `ShortcutRecorder`, etc.
- `keybindings`: `useKeyBindings`, `LAYER`, etc.
- `hooks`: `usePluginSetting`, `usePluginStream`, etc.

Each gets a corresponding shim file in `plugin-sdk/shims/` and
eventually a package entry point in the npm package.

### Once aliases are no longer needed

With a proper npm package, the `resolve.alias` entries in each plugin's
`vite.config.ts` disappear. The Vite plugin in the SDK handles React
shimming. Type imports resolve from `node_modules` normally. The only
remaining special handling is React (always host-provided, never
bundled) — the Vite plugin takes care of that permanently.

## When

After the `plugin-sdk/` directory stabilizes through 1-2 real plugin
migrations. The in-repo alias approach works fine during development —
the npm package is for third-party consumption.

## References

- Current shim approach: `plugin-sdk/shims/` (to be created)
- Host SDK global: `src/lib/sdk.ts`
- Type sources: `src/types.ts`, `src/plugins/types.ts`
- Related: `01knftk3xkj0hgdnj87gk9qzyg-plugin-sdk-crate.md` (Rust side)
