---
kind: chore
severity: medium
status: needs-discussion
area: [assets/mascot, just/assets.just, tools, .github/workflows/ci.yml]
tags: [tooling, ci]
---

# Move the mascot source PNGs to an external repository

Decided: the base mascot assets move to a separate repository and are
removed from this repository's history. The open questions below need
answers before the move.

## Numbers

- `git count-objects`: the pack is 219 MiB.
- Source PNG blobs under `assets/mascot/` across all history: 220
  blobs, 195.7 MiB. The PNGs are already compressed, so git stores
  every version in full.
- Current sources: 189 `snappy-*-1024.png` files, 165.3 MiB, plus
  `snappy-original-2048.png` (1.4 MiB), which no recipe reads.
- The bundled WebPs in `public/images/mascot/` stay here: 567 files,
  10.2 MiB, 11.9 MiB across history.
- 500 more mascots at the current average of 895 KiB per source would
  add about 437 MiB.

## What reads the sources today

- `asset-mascots` (`just/assets.just:18`) renders the 96/192/384 WebPs.
- `asset-mascot-data` (`just/assets.just:63`) measures trim values from
  the source alpha channel into the gitignored
  `src/derived/mascots.json`. `build`, `build-frontend`, `start`,
  `check-types` and `test-frontend` depend on it, and CI runs it
  through `just install`.
- `asset-app-icons` (`just/assets.just:137`) composes the app icon from
  `assets/mascot/snappy-original-1024.png`. The generated icons,
  `app-icon-source.png` included, are gitignored.
- `tools/normalize-mascot-size`, `tools/mascot-size-sheet`,
  `tools/detect-mascot-borders`, `tools/detect-mascot-strays`,
  `tools/refine-mascot-alpha`.
- Docs that describe the source paths: `assets/mascot/docs/Adding-a-Mascot.md`,
  `CLAUDE.md` ("Mascots"), `src/derived/README.md`, and the missing-trim
  warning in `src/mascotVariants.ts`.

## Open questions

- Trim data without sources: commit `src/derived/mascots.json` (or the
  trim values) and regenerate it from the asset repository, or measure
  trim from the 384 px WebPs here.
- App icon: keep `snappy-original-1024.png` here, commit the composed
  `app-icon-source.png`, or read it from the asset repository.
- What moves with the PNGs: the `tools/` scripts, `assets/mascot/docs/`,
  the hand-authored `assets/mascot/mascots.json`.
- How this repository finds the asset repository when regenerating
  WebPs: a sibling checkout like `../torchsnap-docs`, a submodule, or
  an explicit path argument.
- How WebPs get from the asset repository into `public/images/mascot/`.

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

- `todos/build/01m3hr33w2v237ba8xhsyceaq4-cache-mascot-trim-values.md`
- `todos/build/01m3hr33w2v237ba8xhsyceaq6-mascot-asset-regeneration-scope.md`
- `todos/build/01m3hrcngf3qpw1pf3jf3k1ypj-mascot-download-cost-in-updates.md`
