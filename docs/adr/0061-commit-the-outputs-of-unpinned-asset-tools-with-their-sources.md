# 61. Commit the outputs of unpinned asset tools with their sources

Date: 2026-10-02

## Status

Accepted

## Context

Several assets are made from sources by tools, and some of their
outputs are tracked while others are not:

- The app icon candidates in `assets/app-icon/` are written by
  `tools/flatten-emblem` and `tools/build-app-icons`, which use
  `rsvg-convert`, ImageMagick and Icon Composer's `ictool`;
  `just app-icon-compile` writes `assets/app-icon/compiled/` with
  `actool` and `ictool` (ADR 60). All of them are tracked.
- `just asset-app-icons` fans the compiled icon out into
  `src-tauri/icons/` through `tauri icon`; those files are listed in
  `.gitignore`. `bun.lock` pins `@tauri-apps/cli` to 2.11.5.

The DMG window gets a background picture. Tauri's DMG bundling takes it
from `bundle.macOS.dmg.background`. Finder draws it from the top left of
the window's content at one image pixel per point, and from a TIFF that
holds a 1x and a 2x image it draws the 2x one on Retina displays. On a
background picture Finder writes the icon labels in black. The view
settings it stores for the window have no label colour, and a dark
background colour under the picture did not change it, so the picture
has to be light behind the labels.

The maintainer chose a picture drawn by an image model
(`gpt-image-2.5-sunburst`, images edit, 2026-10-02): a winter night with
aurora over a snowfield and a campfire between the icons, made from a
vector scene and a generated campfire draft. The same result cannot be
guaranteed from the model again. The TIFF is made from it with `sips`
and `tiffutil`, which come with macOS; their exact output across macOS
and tool versions cannot be guaranteed either.

## Decision

As a guideline, outputs of tools whose version the repository pins
(`bun.lock`, `Cargo.lock`) are built during the build and not
committed; outputs of tools it does not pin (macOS built-ins, Xcode,
Homebrew tools, image models) are committed together with their
sources. It is not a strict rule: each case is decided with logic and
pragmatism.

The app icon stays as ADR 60 has it: the candidates and `compiled/` are
tracked, the fan-out in `src-tauri/icons/` is not.

The DMG background lives in `assets/dmg-background/`, next to
`assets/app-icon/`:

- a README with the picture's provenance: model, date, prompts, inputs
  and the steps from the model's image to the TIFF;
- the model's raw image (1536 × 1024);
- `inputs/` with the vector scene, the campfire draft and both prompts;
- the TIFF with a 1x (640 × 468) and a 2x (1280 × 936) image, committed
  as `tiffutil` writes it (LZW with predictor). It is not optimised
  further: Deflate at level 9 through libtiff's `tiffcp` made it 2.2 %
  smaller, for an additional Homebrew dependency;
- a generator script under `tools/` for the scene and the TIFF.

The repository has no script that calls the image model: none that
reads an OpenAI key or spends money. The README documents the model,
the prompts and the inputs instead.

The DMG window is 640 × 500 points, with the app at (170, 260) and the
Applications folder at (470, 260); `bundle.macOS.dmg.background` names
the committed TIFF. The other picture candidates are not kept in the
repository.

## Consequences

Every DMG build uses the committed TIFF, whatever the macOS version of
the machine that builds it.

Changing the DMG background means a new picture, a new TIFF from the
generator script, and a commit of both. The model's image is kept as
the model returned it; its README is the record of how it was made.
