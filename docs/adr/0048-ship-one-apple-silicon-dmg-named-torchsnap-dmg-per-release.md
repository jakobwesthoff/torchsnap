# 48. Ship one Apple silicon DMG named Torchsnap.dmg per release

Date: 2026-09-23

## Status

Accepted

## Context

The download button on torchsnap.app links
`https://github.com/jakobwesthoff/torchsnap/releases/latest/download/Torchsnap.dmg`.
GitHub redirects `releases/latest/download/<name>` to the asset called
`<name>` in the latest release, and answers with a 404 when that release
has no such asset.

Tauri names the DMG `<productName>_<version>_<arch>.dmg`. With
`productName` set to `torchsnap`, the bundle was `torchsnap.app` and the
DMG `torchsnap_0.9.3_aarch64.dmg`. Everywhere else the product is called
Torchsnap.

`just build --release` on an Apple silicon Mac produces an `arm64`
binary. `rust-toolchain.toml` lists no `x86_64-apple-darwin` target, and
no recipe builds for Intel.

## Decision

Every release carries one DMG, uploaded under the name `Torchsnap.dmg`.
It contains an `arm64` build for Macs with Apple silicon. There is no
universal and no separate Intel DMG.

`productName` in `src-tauri/tauri.conf.json` is `Torchsnap`, so the
bundle is `Torchsnap.app` and the local DMG
`Torchsnap_<version>_aarch64.dmg`. The bundle identifier stays
`app.torchsnap` and the executable inside the bundle stays `torchsnap`.

## Consequences

- The release process has to upload the DMG as `Torchsnap.dmg`, not
  under the name Tauri gives it.
- The asset name carries neither version nor architecture, so the
  website's link works for every release.
- Macs with an Intel processor cannot run the release.
- Settings and installed gadgets live under directories named after the
  bundle identifier, so they carry over from `torchsnap.app` to
  `Torchsnap.app`.
