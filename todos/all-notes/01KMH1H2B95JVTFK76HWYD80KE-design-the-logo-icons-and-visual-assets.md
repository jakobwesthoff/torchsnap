---
title: "Design the logo, icons and visual assets"
kind: feature
component: project
---
# Design the logo, icons and visual assets

Create the visual identity assets for torchsnap based on the direction
set by an earlier visual identity brainstorm. That brainstorm has no
todo left in the vault, so its direction has to be recovered before the
design work starts.

## Assets needed

### App icon
- macOS: `.icns` (1024x1024 master, all required sizes)
- Windows: `.ico` (16, 32, 48, 256)
- Linux: SVG preferred, PNG fallbacks at standard sizes
- Should work at all sizes from 16x16 (tiny favicon) to
  1024x1024 (macOS full-size icon)

### Tray icon
- macOS: monochrome template image (22x22pt, @1x and @2x).
  Must work as a menubar template (system inverts for dark
  menubar automatically).
- Linux: full-color SVG or PNG (varies by DE)
- Windows: `.ico` at 16x16 and 32x32

### Mascot (if decided)
- Multiple sizes for different contexts (launcher overlay,
  about screen, marketing, documentation)
- WebP and PNG exports
- Both light and dark background variants

### Marketing / web
- Social preview image (1280x640 for GitHub/OpenGraph)
- Banner for README
- Favicon for docs site (if applicable)

## Process

1. Resolve the direction from the earlier visual identity brainstorm
2. Rough sketches / concepts (3 to 5 directions)
3. Refine chosen direction
4. Export all required sizes and formats
5. Replace placeholder icons in `src-tauri/icons/`
6. Update tray icon `include_bytes!` path in `src-tauri/src/lib.rs`
7. Test at all sizes on all platforms

## Tools

- Vector work: Figma, Illustrator, or Inkscape
- Icon generation: `bunx tauri icon <source.png>` generates all
  platform variants from a 1024x1024 source
