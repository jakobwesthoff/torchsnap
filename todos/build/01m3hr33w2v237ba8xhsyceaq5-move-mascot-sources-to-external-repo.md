---
kind: chore
severity: medium
status: open
area: [git history, assets/mascot]
tags: [tooling]
---

# Remove the old mascot source PNGs from the history

Decided: the base mascot assets move to a separate repository and are
removed from this repository's history. The move itself is done (ADR
57): the mascot set is made in torchsnap-mascot, whose export writes the
WebPs and `src/derived/mascots.json` here, and the source PNGs, their
data, docs and tools are gone from the tree. What is left is the
history.

## Numbers (2026-09-27, before the move)

- `git count-objects`: the pack is 219 MiB.
- Source PNG blobs under `assets/mascot/` across all history: 220
  blobs, 195.7 MiB. The PNGs are already compressed, so git stores
  every version in full.
- The bundled WebPs in `public/images/mascot/` stay: they are what the
  app ships.

## What stays in the tree

`assets/mascot/snappy-original-1024.png`, the app icon source (a copy
of torchsnap-mascot's `masters/original.png`). A history filter must
keep it, or the app icon recipe loses its input.

## History rewrite

- Rewriting history changes every commit hash. The repository has 9
  tags. They have to be rewritten and force-pushed along with `main`,
  and the GitHub releases that hang off them checked afterwards.
- Every existing clone and worktree has to re-clone afterwards.
- Check CHANGELOG, `../torchsnap-docs` and `../torchsnap-web` for
  references to commit hashes before rewriting.
- Do it between releases: `release-publish` refuses when `HEAD` moved
  after `release-build`.

## Related

- `todos/build/01m3hrcngf3qpw1pf3jf3k1ypj-mascot-download-cost-in-updates.md`
