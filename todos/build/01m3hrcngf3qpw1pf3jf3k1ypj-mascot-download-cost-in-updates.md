---
kind: decision
severity: low
status: needs-discussion
area: [just/assets.just, src/mascotVariants.ts, src-tauri/tauri.conf.json]
tags: [performance]
---

# Decide how mascots reach users as the roster grows

Nothing is decided. The options below are candidates for discussion;
staying with the full app bundle as it is today is one of them.

## Numbers

- Bundled cost per mascot (96 + 192 + 384 px WebP): mean 55.2 KiB,
  median 54.8 KiB, range 33.2 to 81.1 KiB. The 384 px file is 37.1 KiB
  of that, the 96 px file 5.0 KiB.
- All 189 mascots: 10.2 MiB in `public/images/mascot/`. Brotli, which
  Tauri applies to embedded assets, saves 0.5% on them.
- Release 0.13.0: `Torchsnap.app.tar.gz` 28.0 MB, `Torchsnap.dmg`
  28.2 MB. Mascots are about 38% of the update archive.
- 500 more mascots add about 27 MiB. The archive would grow to about
  56 MB, with mascots about 69% of it.
- `tauri-plugin-updater` downloads the whole `.app.tar.gz` and
  replaces the app (ADR 0053), so every update carries every mascot.

## Build time and binary size

Measured on 2026-09-27 on an Apple M1 (8 cores) with `hyperfine`,
release profile, `tauri build --no-bundle` with `beforeBuildCommand`
emptied. The 500 extra mascots were copies of existing WebPs with 32
random bytes appended, so each has its own content hash.

| Step | 189 mascots | 689 mascots |
|---|---|---|
| `vite build` | 1.02 s | 1.45 s |
| App crate compile, codegen asset cache warm | 53.0 s ± 1.4 | 57.2 s ± 1.8 |
| App crate compile, codegen asset cache cold | 55.7 s ± 2.8 | 62.4 s ± 0.0 |
| `target/release/torchsnap` | 46.3 MB | 74.9 MB |

- `tauri-codegen` 2.6.3 brotli-compresses every `dist/` file at quality
  9 in release and caches the result in the app crate's `OUT_DIR` under
  the content hash (`embedded_assets.rs`). "Cold" deletes that cache
  before the build, as in a clean target directory. "Warm" only touches
  `src-tauri/src/lib.rs`.
- About 50 s of each compile is `torchsnap_lib` itself. The 500 extra
  mascots add 4.2 s warm and 6.8 s cold.
- The binary grows by 28.6 MB, about the size of the added WebPs.
- Not measured: the bundle step (`.app`, DMG, `.app.tar.gz`).

## Where each size is shown

`Mascot` (`src/components/Mascot.tsx`) loads `size` at 1x and
`size * 2` at 2x. The largest logical size is 192 px (launcher center
mode, welcome launcher preview), so 384 px is the Retina file for it.
96 px is loaded only for sidekick mode and the welcome "You're set"
step on 1x displays.

## Options

### Stay with the full bundle

No change. Size grows linearly with the roster.

### Keep bundling, shrink the files

- Drop the 96 px size: saves 0.93 MiB today. 1x displays would
  downscale the 192 px file in sidekick mode.
- Quality sweep on the 384 px files (`cwebp -q 90 -alpha_q 100` in
  `asset-mascots`). Not yet measured which settings stay visually
  clean.
- AVIF instead of WebP. Not yet measured. WebKit supports AVIF from
  macOS 13; `tauri.conf.json` sets no `minimumSystemVersion`, so this
  needs a raised minimum.

### Mascot packs downloaded separately

- Bundle a core set, download the rest into the app data directory
  and serve it through a custom URI protocol, as the gadget and favicon
  protocols do (`src-tauri/src/lib.rs`, `register_gadget_protocol` and
  `register_favicon_protocol`).
- Packs are versioned apart from the app: a new mascot needs no app
  release, and an app update carries no mascots.
- `Variants` and `SnappyHeroSets` in `src/mascotVariants.ts` are
  hard-coded. Groups, holiday sets, alt text, NSFW flags and trim
  would have to ship as pack data.
- Needs signing and verification, hosting, and a defined first run
  without network.
- Would supply trim values as pack metadata, which touches
  `todos/build/01m3hr33w2v237ba8xhsyceaq5-move-mascot-sources-to-external-repo.md`.

### Delta updates

`tauri-plugin-updater` offers no delta updates that we know of. This
would mean replacing the updater.

## Questions for the discussion

- Are 500 more mascots a plan, or a ceiling being checked?
- With packs: which mascots belong to the core set, whether the app
  must work fully offline with only that set, and whether the rest
  downloads automatically or on opt-in.
- Is raising the minimum macOS version to 13 acceptable for AVIF?

## Related

- `todos/build/01m3hr33w2v237ba8xhsyceaq4-cache-mascot-trim-values.md`
- `todos/build/01m3hr33w2v237ba8xhsyceaq6-mascot-asset-regeneration-scope.md`
