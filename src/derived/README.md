# Derived Data

This directory contains generated files. They should not be edited by
hand — changes are overwritten on the next regeneration.

## Files

| File | Source | Recipe | Tracked |
|---|---|---|---|
| `mascots.json` | the torchsnap-mascot repository | `just export <torchsnap checkout>` there | yes |
| `timezone-coordinates.json` | System `/usr/share/zoneinfo/zone.tab` | `just asset-timezone-data` | yes |

## `mascots.json` comes from torchsnap-mascot

The mascot set (the images in `public/images/mascot/` and this file) is
made and reviewed in the torchsnap-mascot repository, whose export writes
both into a torchsnap checkout. Per mascot the file holds `alt`, `nsfw`,
`group`, `groundAnchor` and `boxLeft`; that repository's
`docs/metadata.md` and the export section of its `docs/pipeline.md`
explain them. Both are tracked here, so a fresh
checkout builds without that repository.

`timezone-coordinates.json` is tracked because its source is a system file
that is not part of the repository.

## Regeneration

```sh
# Regenerate the timezone data
just asset-timezone-data

# The mascot data, from a torchsnap-mascot checkout
just export ../torchsnap
```
