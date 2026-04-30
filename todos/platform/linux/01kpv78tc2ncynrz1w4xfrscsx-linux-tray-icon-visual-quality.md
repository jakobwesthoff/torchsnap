# Linux tray icon visual quality

Discovered while bringing up the project on Fedora 43 + GNOME. Getting
a tray icon to appear at all required a non-trivial sequence:

1. Install the `libayatana-appindicator-gtk3` runtime shared library
   (not `-devel`). Without it the app panics at startup with
   `Failed to load ayatana-appindicator3 or appindicator3 dynamic library`
   because `libappindicator-sys` `dlopen`s it lazily.
2. Install and enable the GNOME extension
   `gnome-shell-extension-appindicator` (UUID
   `appindicatorsupport@rgcjonas.gmail.com`). GNOME Shell itself
   dropped system-tray support in 3.26 and does not subscribe to the
   freedesktop StatusNotifierItem D-Bus spec — the extension is the
   standard bridge. Without it the app runs but the icon is silently
   invisible.

This whole process is documented in
`docs/Howto-build-on-fedora-43.md` §1.1 and §1.2.

## Actual visual issue

Once the icon finally shows up, it's **the full app icon** —
`src-tauri/icons/32x32.png`, which `just/assets.just:asset-app-icons`
composites from the mascot on top of the orange-to-amber torch
gradient. That looks acceptable as an application launcher icon but
is wrong for a top-bar tray slot:

- The gradient background is busy and chromatic, clashing with any
  theme (especially dark-mode top bars).
- At 22×22 rendered size the gradient dominates and the mascot
  becomes an unreadable blob.
- `src-tauri/src/platform/fallback/tray.rs` already carries a
  `TODO` comment acknowledging that a dedicated tray asset optimised
  for small sizes and dark/light system themes is needed.

A `src-tauri/icons/tray-icon-template.png` asset exists but is built
as a macOS template alpha mask (`just/assets.just:asset-tray-icon`,
via `tools/png-to-alpha-mask`). It is not usable as-is on Linux
because GTK/appindicator renders the raw pixels — there is no
template-image concept.

## Options

- **Ship a Linux-specific tray asset** — a small flat mark (mascot
  silhouette or torch glyph) on transparent background, probably as
  SVG so GNOME/KDE scale it cleanly at various top-bar heights.
  Load it conditionally behind `#[cfg(target_os = "linux")]` in
  `tray.rs`.
- **Reuse the template source with a colour fill** — take the same
  44×44 mascot silhouette used for the macOS template, but export a
  version that keeps its alpha and a neutral fill (e.g. white or
  theme-accent) instead of collapsing to an alpha mask.
- **Drop the tray on GNOME entirely** — GNOME HIG position is that
  persistent status icons are an anti-pattern, so a cleaner product
  answer is a `.desktop` launcher + global shortcut + background
  portal. Larger scope; would change product behaviour on every
  Linux desktop, not just GNOME.

Related: ADR-worthy discussion of "is the tray part of the product
on Linux at all?" — but at minimum, while it *is* shipped, the icon
should not be the gradient-on-app-icon.

## Pointers

- `src-tauri/src/platform/fallback/tray.rs:50` — `include_bytes!`
  of `icons/32x32.png`
- `just/assets.just` — `asset-app-icons` builds 32×32 et al.;
  `asset-tray-icon` builds the macOS template
- `todos/01kmgymgjvkr1e6egscdwctf2m-create-dedicated-tray-icon.md`
  — macOS-focused version of the same concern
