---
kind: feature
status: blocked
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
depends-on: [todos/platform/linux/01m399736658afgk6xv4ab68wr-linux-torchsnap-file-association.md]
---

# Linux: register the file type at runtime for AppImage builds

Only relevant if AppImage ships. Add-on to the deb and rpm file
association.

## Why

deb and rpm install the desktop file and the MIME XML system-wide. An
AppImage is one executable the user downloads and runs from anywhere,
with no install step, so nothing registers `.torchsnap`. Unless the
user runs a helper like appimaged or AppImageLauncher, double-click
does nothing.

## Approach

At startup, only when running as an AppImage (the `APPIMAGE`
environment variable holds the image path):

1. Write `~/.local/share/mime/packages/app.torchsnap.xml`, with the
   same content as the system-wide XML in the deb todo.
2. Write `~/.local/share/applications/app.torchsnap.desktop` with
   `Exec="<$APPIMAGE>" %F`, `MimeType=` for the gadget type (and
   later `x-scheme-handler/torchsnap`), and an icon copied to
   `~/.local/share/icons/`.
3. Run `update-mime-database ~/.local/share/mime` and
   `update-desktop-database ~/.local/share/applications`, then
   optionally `xdg-mime default app.torchsnap.desktop <mime-type>`.

## Things to handle

- **The AppImage moves.** The deep-link plugin docs name this problem
  for URL schemes: the registration points at the old path. Compare
  the stored `Exec` path with `$APPIMAGE` on every start and rewrite
  if it changed.
- **Consent and leftovers (decided 2026-09-24).** Registration is on
  by default. A settings toggle turns it off, and a "Remove desktop
  integration" action deletes the written files, since deleting the
  AppImage leaves them behind.
- **Helpers.** appimaged and AppImageLauncher write their own desktop
  files. Two entries for the same app show up twice in "Open With".
  Detect them, or at least document it.
- **Overlap with deep-link.** The deep-link plugin's `register_all()`
  already does runtime registration for URL schemes on Linux. Check
  whether it writes its own desktop file, so we do not end up with
  two competing ones once the URL scheme lands.

## Done when

- A fresh AppImage, after its first start, opens
  `.torchsnap` files on double-click.
- Moving the AppImage and starting it again fixes the registration.
