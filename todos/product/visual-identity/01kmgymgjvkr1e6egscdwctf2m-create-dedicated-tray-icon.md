# Create dedicated tray icon

Currently using `32x32.png` (the app icon) as the tray icon via
`include_bytes!`. This works but doesn't follow macOS menubar
conventions:

- macOS tray icons should be template images (monochrome, ~22x22
  points / 44x44 pixels @2x)
- The `icon_as_template(true)` call is already in place, but the
  source image has color and is too large

## What needs to happen

1. Design a monochrome tray icon (torch/flame motif) at 22x22pt
2. Export as `tray-icon-template.png` (1x) and
   `tray-icon-template@2x.png` (2x) into `src-tauri/icons/`
3. Update `include_bytes!` path in `lib.rs`

## Platform considerations

- Linux: Tray icons should be full-color SVG or PNG (no template
  concept). May need separate assets per platform.
- Windows: `.ico` format, typically 16x16 and 32x32.
