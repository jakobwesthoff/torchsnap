# Derived Data

This directory contains generated files. They should not be edited by
hand — changes are overwritten on the next regeneration.

## Files

| File | Source | Recipe | Tracked |
|---|---|---|---|
| `mascots.json` | the torchsnap-mascot repository | `just export <torchsnap checkout>` there | yes |
| `mascot-groups.json` | the torchsnap-mascot repository | `just export <torchsnap checkout>` there | yes |
| `timezone-coordinates.json` | System `/usr/share/zoneinfo/zone.tab` | `just asset-timezone-data` | yes |

## The mascot data comes from torchsnap-mascot

The mascot set (the images in `public/images/mascot/`, `mascots.json`
and `mascot-groups.json`) is made and reviewed in the torchsnap-mascot
repository, whose export writes them into a torchsnap checkout. Per
mascot `mascots.json` holds `alt`, `nsfw`, `group`, `occasions`,
`groundAnchor` and `boxLeft`; per group `mascot-groups.json` holds
`label`, `description` and `seasonal`. That repository's
`docs/metadata.md` and the export section of its `docs/pipeline.md`
explain them. All are tracked here, so a fresh checkout builds without
that repository.

`timezone-coordinates.json` is tracked because its source is a system file
that is not part of the repository.

## Regeneration

```sh
# Regenerate the timezone data
just asset-timezone-data

# The mascot data, from a torchsnap-mascot checkout
just export ../torchsnap
```
