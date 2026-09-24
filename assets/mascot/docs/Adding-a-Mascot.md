## Adding a Mascot

How to prepare a new Snappy image and wire it into the app. Names and
descriptions follow [Description-and-Naming-Rules.md](Description-and-Naming-Rules.md).

### 1. The source image

- Save it as `assets/mascot/snappy-<name>-1024.png`, where `<name>` is
  the variant key (with the `-nsfw` suffix when it applies).
- Exactly **1024×1024** pixels, RGBA, transparent background.
  `just asset-mascots` and `just asset-mascot-data` stop with an error
  on any other size: the launcher draws every variant in a square box,
  and the trim values are percentages of a 1024×1024 canvas.
- A drawing that arrives on a taller or wider canvas has to be fitted
  onto a square one. Crop it to its drawing and scale it **down** to
  fit; never enlarge.

### 2. A clean alpha matte

- `tools/detect-mascot-borders` lists images with fringe pixels along
  the canvas edges; `tools/refine-mascot-alpha` re-cuts the matte.
- `tools/detect-mascot-strays` lists residue inside the canvas. Residue
  below the feet matters most: the launcher aligns the lowest visible
  pixel, so a stray scrap there makes Snappy float.

### 3. Body size

The launcher draws every variant at the same pixel size (192 px, 96 px
in sidekick mode), so the size of the owl on the canvas is the size the
user sees. Props do not count; the owl does.

**No source changes size before the maintainer has approved it on a
comparison sheet.** The measurement below only proposes; the decision
is visual.

1. Propose sizes into a separate directory, leaving the sources as they
   are:

   ```sh
   tools/normalize-mascot-size --output-dir /tmp/mascot-proposed assets/mascot/snappy-<name>-1024.png
   ```

   The tool finds the two pupils and compares their distance with the
   median of the existing variants (221.5 px; `original` has 232.7).
   When the owl is more than 3% larger, it writes a scaled-down version
   with the feet where they were. It never enlarges: an owl drawn too
   small needs a new drawing, not an upscale.
2. Check that it measured the pupils (`--dry-run` prints only the
   numbers). It can pick up other dark round shapes (the dome panels of
   `brave-little-astromech`), and it finds nothing behind sunglasses,
   visors or masks. In that case read the pupil centres off the image
   and pass them with `--eyes X1,Y1,X2,Y2`.
3. Draw the comparison sheet and hand it to the maintainer:

   ```sh
   tools/mascot-size-sheet -o /tmp/mascot-sizes.png --proposed-dir /tmp/mascot-proposed assets/mascot/snappy-<name>-1024.png
   ```

   Each row shows reference variants of the right size, the image as
   it is, and the proposed version, all drawn like the launcher with
   the feet on one baseline. Add more references with `--reference`
   when a costume needs a closer comparison.
4. Apply only what was approved: copy the approved proposals over
   their sources. For stylized faces (a mask, oversized glasses, a
   wide-set animal face) the pupil distance can overstate the owl's
   size, and the sheet may show the unchanged image as the right one;
   those stay as they are.

### 4. Optimization

Run every source PNG that was created or changed through

```sh
oxipng -o max --strip safe assets/mascot/snappy-<name>-1024.png
```

The existing sources are already optimized. A second `oxipng` run must
report nothing left to save.

### 5. Registration

1. Add the variant to `assets/mascot/mascots.json` with its `alt` text
   and `nsfw` flag.
2. Add the key to its group in `src/mascotVariants.ts` (seasonal
   variants go into their holiday group only, so they appear just in
   that window).

### 6. Generated files

```sh
just asset-mascots asset-mascot-data
```

- `asset-mascots` writes `public/images/mascot/snappy-<name>-{96,192,384}.webp`.
  These are committed.
- `asset-mascot-data` writes `src/derived/mascots.json` with the trim
  values. It is gitignored and also runs before `just build` and
  `just start`.

### 7. Commit

- Commit the source PNG, its three WebPs, the `mascots.json` and
  `src/mascotVariants.ts` changes, and a CHANGELOG entry.
- Commit only images whose pixels changed. `asset-mascots` only
  rewrites the WebPs of sources that are newer than them, so re-running
  it leaves the other images untouched.
