---
kind: improvement
status: open
---

# Tray icon in a light and a dark variant instead of a template

The macOS tray icon is a template image: `platform/macos/tray.rs` loads
`src-tauri/icons/tray-icon-template.png` with `icon_as_template(true)`,
and `just asset-tray-icon` builds that file from
`assets/snappy-tray-template-2-44px.png`. macOS draws a template from
its opacity only, dark on a light menubar and light on a dark one.

`assets/snappy-tray-template-3.svg` (with 44 and 128 px renders) is the
Snappy emblem (`assets/snappy-emblem-feathered.svg`) in grey levels:
each colour's luminance, stretched so the cream face is fully clear and
the darkest tone fully opaque; the forehead cap's dark brown takes the
head's black, so the cap reads as part of the head (user). On a light
menubar it shows the owl with
shaded feathers. As a template on a dark menubar it shows the negative:
white head, dark face, white eyes with a dark highlight.

`assets/snappy-tray-dark.svg` is the matching dark variant: white, with
opacity equal to the stretched luminance, so the face is opaque and the
dark head and eyes are clear. On a dark menubar it shows a white owl
face with dark eyes.

Both were made by flattening the emblem's stacked shapes (each keeps
only the part no later shape covers) and uniting the parts of equal
tone, so no semi-transparent shapes overlap.

## Decision so far

2026-09-30: the user likes the two-variant look best (the template-3
art on a light menubar, the dark variant on a dark one) but does not
want to build it now. Template 3 is meant as the template for the time
being; switching the build from template 2 to template 3 is a separate
step.

## What the change involves

- Load the icon without template mode and set the light or the dark
  variant depending on the menubar's appearance, updating it when the
  appearance changes.

## Open questions

- What decides the menubar's appearance: only the system's light or
  dark mode, or also the wallpaper? If the latter, following the
  system theme is not enough and the status item's own appearance has
  to be observed.
- Can that appearance be observed through Tauri, or does it need
  native code for the status item?
- How should the icon look while its menu is open?
