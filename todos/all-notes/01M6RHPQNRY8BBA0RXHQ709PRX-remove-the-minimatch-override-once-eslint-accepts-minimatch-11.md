---
title: "Remove the minimatch override once eslint accepts minimatch 11"
kind: chore
component: project
status: blocked
impact: low
origin: upstream
tags: [dependencies]
---
# Remove the minimatch override once eslint accepts minimatch 11

The root `package.json` forces minimatch `^11.0.1` through `overrides`, to
clear the moderate advisory GHSA-g3hv-75wg-m249 in minimatch 10.2.6. Added
in the dependency round of 2026-11-06. eslint 10.14.0, `@eslint/config-array`
0.23.5 and `@typescript-eslint/typescript-estree` 8.72.1 still ask for
`^10`, so they run on a major they do not declare. Only dev tooling is
affected: `bun why minimatch` shows no path into the shipped app.

## Scope

Delete `overrides.minimatch` once those packages require minimatch 11.

## Done when

- `just lint` and `just audit` pass without the override.

## Relations

- Blocked by: eslint and typescript-eslint releases that accept minimatch 11
