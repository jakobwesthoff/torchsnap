# Derived Data

This directory contains **generated files that are checked into version
control**. They are produced by `just` recipes from source data and should
not be edited by hand — your changes will be overwritten on the next
regeneration.

## Files

| File | Source | Recipe |
|---|---|---|
| `mascots.json` | `assets/mascot/mascots.json` + `assets/mascot/*-1024.png` | `just asset-mascot-data` |
| `timezone-coordinates.json` | System `/usr/share/zoneinfo/zone.tab` | `just asset-timezone-data` |

## Why check them in?

These files are needed at build time by Vite's JSON import. Checking them
in means `bun install && bun run build` works without requiring ImageMagick
or other asset tooling to be installed — only contributors who change the
source data need to regenerate.

## Regeneration

```sh
# Regenerate all derived data
just assets

# Or individually
just asset-mascot-data
just asset-timezone-data
```
