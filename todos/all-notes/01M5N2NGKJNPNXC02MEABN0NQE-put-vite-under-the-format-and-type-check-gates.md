---
title: "Put vite/ under the format and type-check gates"
kind: chore
component: project
origin: implementation
tags: [tooling]
---
# Put vite/ under the format and type-check gates

No gate formats or type-checks the TypeScript under `vite/` (`vite/csp.ts`, `vite/keep-mtimes.ts` and their tests). Found on 2026-10-23 while adding `vite/keep-mtimes.ts`; its author ran tsc, eslint and prettier on it by hand.

## Problem

- `package.json` `fmt:check` runs `prettier --check 'src/**/*.{ts,tsx,css}' '*.{js,ts,json}'`, which matches neither `vite/*.ts` nor files below it.
- `just check-types` runs `bun run tsc --noEmit` against `tsconfig.json`, whose `include` is `["src"]`.
- `tsconfig.node.json` includes `vite.config.ts`, `vitest.config.ts` and `vite/**/*.ts`, but no recipe runs it.

`eslint .` in `just lint` does cover `vite/`.

## Suggested fix

Add `'vite/**/*.ts'` to the `fmt` and `fmt:check` globs, and a `tsc --noEmit -p tsconfig.node.json` step to `just check-types`. Run both once and fix what they report before wiring them in.
