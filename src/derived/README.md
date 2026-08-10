# Derived Data

This directory contains generated files produced by `just` recipes from
source data. They should not be edited by hand — changes are overwritten on
the next regeneration.

## Files

| File | Source | Recipe | Tracked |
|---|---|---|---|
| `mascots.json` | `assets/mascot/mascots.json` + `assets/mascot/*-1024.png` | `just asset-mascot-data` | no |
| `timezone-coordinates.json` | System `/usr/share/zoneinfo/zone.tab` | `just asset-timezone-data` | yes |

## `mascots.json` is generated, not tracked

Its trim values are derived from the alpha channel of the source PNGs, so
adding a mascot changes the file without anyone editing it. It is gitignored
and every recipe that compiles or type-checks the frontend (`just build`,
`just build-frontend`, `just start`, `just check-types`) depends on
`asset-mascot-data`, which regenerates it whenever a source PNG or the
hand-authored `assets/mascot/mascots.json` is newer than the output.

This makes ImageMagick a build requirement. `bun run build` on its own will
fail to resolve the import on a fresh checkout — go through `just` instead.

`timezone-coordinates.json` is tracked because its source is a system file
that is not part of the repository.

## Regeneration

```sh
# Regenerate all derived data
just assets

# Or individually
just asset-mascot-data
just asset-timezone-data
```
