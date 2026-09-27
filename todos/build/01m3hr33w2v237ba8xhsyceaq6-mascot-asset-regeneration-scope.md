---
kind: investigation
severity: low
status: open
area: [just/assets.just]
tags: [performance, tooling]
---

# Regenerate only the mascot assets whose inputs changed

Investigate whether the asset recipes regenerate exactly the outputs
whose inputs changed: nothing more (wasted work, spurious diffs in
tracked files) and nothing less (stale outputs).

## Current behavior

- `asset-mascots` (`just/assets.just:18`) compares mtimes per source
  and size: a WebP is re-encoded when it is missing or older than its
  source PNG. On the current checkout no WebP is older than its source.
- `asset-mascot-data` (`just/assets.just:63`) re-measures every source
  when any single input is newer than the output. Covered by
  `todos/build/01m3hr33w2v237ba8xhsyceaq4-cache-mascot-trim-values.md`.
- `asset-app-icons` (`just/assets.just:137`) is mtime-based in two
  stages.

## Questions

- Git does not store mtimes and writes the files a checkout, pull,
  rebase or branch switch touches with the current time. Can that
  leave a PNG newer than its unchanged WebPs and trigger a re-encode?
- Does a re-encode with a different ImageMagick or `cwebp` version
  produce different bytes? The WebPs are tracked, so that would show
  up as modified files, and `release-build` refuses a build that
  changed tracked files. `asset-mascots` runs through `just assets`
  and `just install`, not through `just build`.
- The encoder settings in the recipe (`-resize`, `cwebp -q 90 -m 6
  -alpha_q 100 -sharp_yuv`) are not an input to the mtime check.
  Changing them regenerates nothing until each source is touched.
- Would content hashes of the source and of the settings, stored next
  to the outputs, fix both directions?

## Related

`todos/build/01m3hr33w2v237ba8xhsyceaq5-move-mascot-sources-to-external-repo.md`
moves the sources to another repository, which changes where these
recipes read from.
