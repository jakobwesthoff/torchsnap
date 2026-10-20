---
title: "Use a dedicated tray icon asset on Linux"
kind: improvement
component: platform
---

# Use a dedicated tray icon asset on Linux

The Linux tray icon is the full app icon: the mascot on an orange-to-amber
torch gradient. That suits an application launcher icon but is wrong for
a top-bar tray slot. Found while bringing the project up on Fedora 43 with
GNOME.

## Getting an icon to appear on GNOME

Getting a tray icon to appear at all required a non-trivial sequence:

1. Install the `libayatana-appindicator-gtk3` runtime shared library
   (not `-devel`). Without it the app panics at startup with
   `Failed to load ayatana-appindicator3 or appindicator3 dynamic library`
   because `libappindicator-sys` `dlopen`s it lazily.
2. Install and enable the GNOME extension
   `gnome-shell-extension-appindicator` (UUID
   `appindicatorsupport@rgcjonas.gmail.com`). GNOME Shell itself dropped
   system-tray support in 3.26 and does not subscribe to the
   freedesktop StatusNotifierItem D-Bus spec, so the extension is the
   standard bridge. Without it the app runs but the icon is silently
   invisible.

This whole process is documented in `docs/Howto-build-on-fedora-43.md`
§1.1 and §1.2.

## Visual issue

Once the icon shows up, it is the full app icon,
`src-tauri/icons/32x32.png`, which `just/assets.just:asset-app-icons`
composites from the mascot on top of the gradient. Problems:

- The gradient background is busy and chromatic, clashing with any theme
  (especially dark-mode top bars).
- At 22x22 rendered size the gradient dominates and the mascot becomes an
  unreadable blob.
- `src-tauri/src/platform/fallback/tray.rs` already carries a `TODO`
  comment acknowledging that a dedicated tray asset optimised for small
  sizes and dark/light system themes is needed.

A `src-tauri/icons/tray-icon-template.png` asset exists but is built as a
macOS template alpha mask (`just/assets.just:asset-tray-icon`, via
`tools/png-to-alpha-mask`). It is not usable as-is on Linux because
GTK/appindicator renders the raw pixels, and there is no template-image
concept there.

## Options

- Ship a Linux-specific tray asset: a small flat mark (mascot silhouette
  or torch glyph) on a transparent background, probably as SVG so
  GNOME/KDE scale it cleanly at various top-bar heights. Load it
  conditionally behind `#[cfg(target_os = "linux")]` in `tray.rs`.
- Reuse the template source with a colour fill: take the same 44x44
  mascot silhouette used for the macOS template, but export a version
  that keeps its alpha and a neutral fill (e.g. white or theme-accent)
  instead of collapsing to an alpha mask.
- Drop the tray on GNOME entirely. The GNOME HIG position is that
  persistent status icons are an anti-pattern, so a cleaner product
  answer is a `.desktop` launcher, a global shortcut and the background
  portal. This is a larger scope and would change product behaviour on
  every Linux desktop, not just GNOME.

Related: an ADR-worthy discussion of "is the tray part of the product on
Linux at all?" is open. At minimum, while the tray ships, the icon should
not be the gradient-on-app-icon.

## Pointers

- `src-tauri/src/platform/fallback/tray.rs`, `FallbackTray`'s tray build: `include_bytes!` of
  `icons/32x32.png`.
- `just/assets.just`: `asset-app-icons` builds 32x32 et al.;
  `asset-tray-icon` builds the macOS template.
- A macOS-focused version of the same concern was completed and removed
  on 2026-09-23.
