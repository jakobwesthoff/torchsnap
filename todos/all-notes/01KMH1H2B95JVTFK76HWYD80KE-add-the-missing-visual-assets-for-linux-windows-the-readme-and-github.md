---
title: "Add the missing visual assets for Linux, Windows, the README and GitHub"
kind: feature
component: project
---
# Add the missing visual assets for Linux, Windows, the README and GitHub

Torchsnap's visual identity exists: the app icon (ADR 0060), the macOS tray template, the mascot set, and the favicons and OpenGraph image of torchsnap.app and the docs site. Four assets are still missing: a Linux SVG app icon, dedicated tray icons for Linux and Windows, a README banner and a custom GitHub social preview.

## Scope

- **Linux app icon as SVG.** `src-tauri/icons/` has PNG sizes, `icon.icns` and `icon.ico` from the app icon candidate in `assets/app-icon/`, but no SVG.
- **Tray icons for Linux and Windows.** The fallback tray (`src-tauri/src/platform/fallback/tray.rs`) loads the 32x32 app icon. Linux wants a full-color SVG or PNG (it varies by desktop environment), Windows an `.ico` at 16x16 and 32x32.
- **README banner.** The README opens with text only.
- **GitHub social preview.** The repository uses GitHub's generated card (1280x640). torchsnap.app's `og.png` (torchsnap-web: `web/public/og.png`, built by `web/tools/build-og.tsx`) may serve as a base.

## Done when

- The four assets exist and are wired up: the SVG and the tray icons in the bundle and the fallback tray, the banner in the README, the preview set in the GitHub repository settings.
- Generated assets follow the repository's asset rules (ADR 0061).
