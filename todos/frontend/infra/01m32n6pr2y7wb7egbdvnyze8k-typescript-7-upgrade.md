# Upgrade to TypeScript 7 once typescript-eslint supports it

**Kind:** dependency upgrade, blocked upstream
**Status:** waiting on typescript-eslint

## Problem

TypeScript 7.0 (the Go-native compiler; 7.0.2 current as of 2026-09-21)
is available, but the lint stack cannot run on it:

- `typescript-eslint` 8.70.1 declares
  `peerDependencies.typescript: ">=4.8.4 <6.1.0"` (npm registry).
- Its type-aware rules crash on TS 7 inside
  `@typescript-eslint/typescript-estree` (typescript-eslint#12518, closed
  as not planned). TS 7.0 has no stable programmatic API; that is planned
  for 7.1.
- `eslint.config.js` applies `tseslint.configs.recommended` to all
  `**/*.{ts,tsx}`, so `bun run lint` and `just fullcycle` would fail.

The project stays on TypeScript 6.0.3, the newest 6.x.

## What is already in place

The tsconfig files use none of the options TS 7 removes (`baseUrl`,
`moduleResolution: node`, `target: es5`, ...): the TS 6 migration
(`d9c1ac7`) already removed `baseUrl` and made `types` explicit. The
upgrade should need no tsconfig changes, but verify.

## When unblocked

1. Check that a typescript-eslint release has a peer range including 7.x.
2. Bump `typescript` together in `package.json` and all five
   `gadgets/*/frontend/package.json` (they pin `^6.0.3` independently).
3. `bunx tsc --version` to confirm the binary in use.
4. `just fullcycle`, each gadget frontend's `bun run typecheck`, and
   `just stage-bundled-gadgets`.
5. Open a `.tsx` file in the editor and confirm the language server
   works.
6. Go/no-go: no "unsupported TypeScript version" warnings from the lint
   stack, clean typecheck, `vite build` output unchanged in size.
