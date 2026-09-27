---
kind: improvement
severity: medium
status: open
area: [just/assets.just, src/derived/README.md]
tags: [performance, tooling]
---

# Cache mascot trim values per source PNG

## Problem

`asset-mascot-data` (`just/assets.just:63`) is all or nothing. When
`assets/mascot/mascots.json` or any single `assets/mascot/*-1024.png` is
newer than `src/derived/mascots.json`, it re-measures the trim of every
source PNG. Each measurement runs `magick identify` for the size check
and again on an alpha-thresholded copy for the bounding box.

Measured on 20 sources: 7.3 s, about 0.36 s per PNG. With the 189
current variants a full run takes about 70 s. At 689 variants (the 500
extra mascots discussed for the roster) it would take about 4 minutes.

The recipe is a dependency of `build`, `build-frontend`, `start`,
`check-types` and `test-frontend`. `src/derived/mascots.json` is
gitignored, so every fresh checkout (including CI, which runs
`just install` and `just fullcycle`) pays for a full run. So does a
pull that brings in one new or changed mascot.

## Suggested fix

Keep a gitignored cache of trim values per source PNG and re-measure
only the PNGs whose cache entry is missing or stale, then merge the
cached values into `src/derived/mascots.json` with `jq` as today. Remove
cache entries for PNGs that no longer exist.

Open: the cache key. mtime is what the other asset recipes use; a
content hash does not change when git rewrites a file's mtime on
checkout.

## Related

`todos/build/01m3hr33w2v237ba8xhsyceaq5-move-mascot-sources-to-external-repo.md`
removes the source PNGs from this repository. Trim values are measured
from those PNGs, so that todo decides where trim data comes from
afterwards and may replace this cache.
