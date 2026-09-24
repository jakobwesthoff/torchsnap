---
kind: feature
status: deferred
area: [src-tauri/src/updates/mod.rs, src-tauri/src/updates/location.rs, tools/release-feed, just/release.just]
tags: [linux, windows]
---

# Updates on Linux and Windows

Torchsnap 0.12.0 updates itself on macOS through `tauri-plugin-updater`
and a feed at `https://torchsnap.app/updates/latest.json` (ADR 0053).
The feed carries one platform entry, `darwin-aarch64`. Linux and
Windows releases do not exist yet; this todo holds what was found about
them while building the macOS updater, so it waits for those releases.

## Linux

- The plugin only installs updates for AppImages.
- Packages from `.deb`, `.rpm` or Flatpak are updated by their package
  manager, so an in-app install would be wrong there. One option: the
  update window only notifies and links to the download page for those
  formats.
- Which formats Linux releases ship is not decided (`todos/platform/linux/`).
- The welcome window's update question would then read "tell me about
  new versions" for the package formats.

## Windows

- The plugin installs MSI or NSIS packages, with
  `plugins.updater.windows.installMode` (`passive`, `basicUi`, `quiet`).

## What the macOS updater already has

- `release.json` (written by `tools/release-feed`) takes more
  `platforms` entries; the plugin looks up `<os>-<arch>-<bundle>` first,
  then `<os>-<arch>`.
- `updates::location` knows only macOS places that cannot be updated
  (a mounted DMG, App Translocation).
- `just release-build` builds and checks the macOS archive only.
