# DMG background

The picture behind the two icons in Torchsnap's DMG window: a winter
night with an aurora over a snowfield and a campfire between the icons.
What is tracked here and why: ADR 61.

## Files

| Path | What |
|---|---|
| `dmg-background.tiff` | The background as the DMG uses it (`bundle.macOS.dmg.background`): a 1x (640 × 468) and a 2x (1280 × 936) image of the window's content. Made by `tools/build-dmg-background tiff`. |
| `model-output.png` | The image model's picture (1536 × 1024), as the model returned it. The source of the TIFF. |
| `inputs/scene.svg` | The night sky, aurora and snowfield the model was given to draw into. Made by `tools/build-dmg-background scene`. |
| `inputs/campfire-draft.png` | The campfire the model was given as a draft (1024 × 1024, transparent). |
| `inputs/prompt-picture.txt` | The prompt of the picture. |
| `inputs/prompt-campfire-draft.txt` | The prompt of the campfire draft. |

## The window

The window is 640 × 500 points (`bundle.macOS.dmg.windowSize`). Finder's
title bar takes the top 32 points; the content below it, 640 × 468, is
where the background is drawn, from its top left corner at one image
pixel per point. On a Retina display Finder draws the TIFF's 2x image.

The icons are centred at (170, 260) and (470, 260) in the content's
coordinates. Finder writes their labels in black 84 points below the
icon centres, so the picture is light there: the labels sit on the
snow. A user who has switched on Finder's path bar sees its 28 points
over the bottom of the content.

`tauri build` lays the window out by scripting Finder. Where Finder's
scripting is blocked, the icon positions and the window size still come
out, but the icon size, the label size and the background picture are
missing: seen when the build ran inside Claude Code's sandbox.

## How the picture was made (2026-10-02)

The model was `gpt-image-2.5-sunburst` through OpenAI's Images API, with
quality `high` and PNG output.

1. The campfire draft: a generation from `inputs/prompt-campfire-draft.txt`,
   1024 × 1024, transparent background; one of four drafts, picked by
   the maintainer.
2. The scene: `inputs/scene.svg` rendered with `rsvg-convert` at 1680 ×
   1336 and cropped to its middle 1280 × 936 (the scene reaches 100
   points past the window on each side). The model's canvas is 3:2, so
   the render was widened to 1404 × 936 by stretching its outermost
   column 62 px out on each side, and scaled to 1536 × 1024.
3. The picture: an edit with that scene and the campfire draft as the
   two input images and `inputs/prompt-picture.txt` as the prompt, size
   1536 × 1024, background `opaque`. Its result is `model-output.png`.

The repository has no script that calls the model (ADR 61); the steps
above repeat it by hand, though the same picture is not guaranteed.

## Rebuilding

```
just dmg-background
```

It writes `inputs/scene.svg` and `dmg-background.tiff`. The TIFF step
scales `model-output.png` to 1404 × 936, keeps the middle 1280 × 936
(undoing the widening of step 2), scales that to 640 × 468 for the 1x
image, and joins both with `tiffutil`. It needs `sips` and `tiffutil`,
which come with macOS; their output may differ between macOS versions,
which is why the TIFF is tracked. Commit both files after a rebuild.
