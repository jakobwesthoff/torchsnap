---
title: "Publish the gadget SDK to npm as @torchsnap/gadget-sdk"
kind: feature
component: gadget-sdk
---
# Publish the gadget SDK to npm as @torchsnap/gadget-sdk

Third-party gadget authors need to install the TypeScript gadget SDK from npm with `bun add -D @torchsnap/gadget-sdk` instead of cloning the monorepo. The workspace package at `packages/gadget-sdk/` already holds the types, the runtime shims, the Vite plugin and the theme CSS, and in-repo consumers reach it through `file:` dependencies, so in-tree development works. What remains is the external publishing: the package is still `"private": true` (version 0.2.0), and its `exports` point at TypeScript sources instead of built JavaScript with generated `.d.ts` files.

## What the workspace package already has

- Types (`@torchsnap/gadget-sdk`, `src/types/`): `GadgetViewProps`, the logger and settings types and the data types.
- Runtime shims (`./hooks`, `./components`, `./keybindings`, `./utils`, plus the `react` and `react/jsx-runtime` shims), which re-export from `window.__torchsnap`. `src/lib/sdk.ts` in the host sets that global with React, components, keybindings and hooks, and declares its full shape.
- `./vite` (`src/vite/index.ts`): the `torchsnap()` plugin that sets the React shim aliases. Gadget frontends add it to their Vite plugins, as `gadgets/template/frontend/vite.config.ts` does.
- `./testing` (`MockGadgetContextProvider`) and `./theme.css`.

## Scope of the publish

- **Build output**: compile the TypeScript sources to JavaScript and generate `.d.ts` files, so the `exports` map points at built files. Keep the subpaths above.
- **Package metadata**: drop `"private": true`, set the version policy, the license (MPL-2.0 like the rest of the repository), `files`, the registry namespace and the `vite` peer range (`>=8`, optional today).
- **Documentation**: a README for the package, covering the subpaths, the shim architecture below and how a gadget author wires up Vite.
- **Publishing step**: a recipe and a check that the published tarball installs and builds a gadget outside the monorepo.

## Design decisions

### React shim architecture

React is a runtime dependency provided by the host via `window.__torchsnap.React`. Gadgets must NOT bundle their own copy.

The `torchsnap()` Vite plugin aliases `react` and `react/jsx-runtime` to the SDK shims, which are inlined into the gadget bundle (a few lines), producing a self-contained ES module with no unresolved imports. This follows the same pattern as VS Code extensions (where `require("vscode")` is intercepted by the extension host) and Figma gadgets (`figma` global). The npm package keeps this unchanged: the aliases point into the installed package instead of the workspace.

### Types

Gadgets only use `import type` for host types, which is erased at compile time, so no runtime shim or duplication is needed. The npm package ships the types from `src/types/` as generated `.d.ts` files.

## When

After the package's shape is stable through one or two more real gadgets. The in-repo workspace setup works fine during development. The npm package is for third-party consumption.

## References

- Package: `packages/gadget-sdk/` (`package.json` `exports`, `src/shims/`, `src/vite/`, `src/types/`)
- Host SDK global: `src/lib/sdk.ts`
- Related: the Rust-side gadget SDK crate, already shipped at `gadgets/gadget-sdk/`
