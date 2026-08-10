# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Torchsnap is a Tauri 2 desktop launcher for macOS with a sandboxed WebAssembly
gadget system; gadgets ship as single-file `.torchsnap` archives, and the set
bundled with release builds is whitelisted in `gadgets/bundled.toml`.

### Added

- The `app-icon` entry icon: gadgets can show an installed application's real
  icon by passing a platform-native identifier (a macOS bundle id), and the
  launcher renders that application's actual icon. Requires the `icon-cache`
  manifest permission.
- The awake gadget shows Amphetamine's real application icon
  (`com.if.Amphetamine`) on its entries; the backend-error entry keeps the
  bolt icon.
- Quality gates cover the gadgets workspace: new `lint-gadgets`,
  `fmt-gadgets`, and `fmt-check-gadgets` recipes wired into `just lint` /
  `just fmt` / `just fmt-check`; `lint-crates` lints all cargo targets.

### Changed

- `just build`, `just build-frontend`, `just start`, and `just check-types`
  depend on `asset-mascot-data`, so the generated `src/derived/mascots.json`
  is rebuilt from the mascot source PNGs before the frontend compiles.
  ImageMagick is required for these recipes; `bun run build` on its own no
  longer resolves the import on a fresh checkout.

### Fixed

- Mascot placement for the 22 variants added in the tournament batch
  (hellfire specter, wide hat monk, iron fist brawler, spec ops commander and
  their siblings). They carried no trim data, so the launcher positioned them
  as if their artwork filled the image canvas edge to edge and they floated
  up to 29px above the search bar instead of resting on it.
- Dev builds warn on the console when a mascot variant has no trim data
  rather than silently misplacing it.
