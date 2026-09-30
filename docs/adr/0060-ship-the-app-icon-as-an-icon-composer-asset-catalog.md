# 60. Ship the app icon as an Icon Composer asset catalog

Date: 2026-10-01

## Status

Accepted

Amends [57. Deliver the mascot set from the torchsnap-mascot repository](0057-deliver-the-mascot-set-from-the-torchsnap-mascot-repository.md)

## Context

The app icon was the plain Snappy mascot
(`assets/mascot/snappy-original-1024.png`, ADR 57) composited onto an
orange gradient, fanned out by `tauri icon`. The Snappy emblem
(`assets/snappy-emblem-feathered.svg`) gave rise to several app icon
candidates, built as Icon Composer documents in `assets/app-icon/`.
An Icon Composer document carries the dark, clear and tinted
appearances of macOS 26.

Tauri 2 does not bundle Icon Composer documents
([tauri#14207](https://github.com/tauri-apps/tauri/issues/14207)); with
one in `bundle.icon`, bundling fails in `actool`
([tauri#15315](https://github.com/tauri-apps/tauri/issues/15315)).
Apple's `actool` compiles a document into an asset catalog
(`Assets.car`) and an `.icns`. On a stub app bundle, macOS drew the icon
from `Assets.car` once `CFBundleIconName` named it.

## Decision

The app icon is the candidate `aurora-rise`: the colour owl rising over
the bottom edge under teal and violet aurora ribbons. The maintainer
chose it for now; the other candidates stay in `assets/app-icon/`.

`just app-icon-compile` compiles the chosen candidate with `actool` into
`assets/app-icon/compiled/` (`Assets.car`, `AppIcon.icns`) and renders
its Default appearance at 1024 px with Icon Composer's `ictool`
(`app-icon-1024.png`). These files are tracked. `just asset-app-icons`
turns the render into the icons of the other platforms through
`tauri icon`, puts `AppIcon.icns` in place of Tauri's `icon.icns`, and
copies `Assets.car`, which `bundle.resources` puts into the bundle's
Resources. `src-tauri/Info.plist` sets `CFBundleIconName` to `AppIcon`.

The DMG's volume icon is the app icon.

`assets/mascot/snappy-original-1024.png` is no longer the app icon
source and is removed.

## Consequences

Changing the app icon means setting `app_icon` in `just/assets.just`,
running `just app-icon-compile` on a Mac with Xcode and Icon Composer,
and committing `assets/app-icon/compiled/`. Building, including on CI,
needs neither.

`just verify-bundle` checks `CFBundleIconName`, `CFBundleIconFile` and
that `Assets.car` and `icon.icns` are in a built bundle.
