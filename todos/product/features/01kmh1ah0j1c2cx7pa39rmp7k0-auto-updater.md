---
kind: feature
status: needs-discussion
area: [src-tauri/tauri.conf.json, src-tauri/Cargo.toml, just/release.just, src-tauri/src/platform/macos/tray.rs, src/settings/sections/GeneralSection.tsx]
tags: [macos, security, privacy]
---

# Check for and install updates

Torchsnap has no way to tell users that a new version exists. Users
have to notice a release on GitHub or torchsnap.app, download
`Torchsnap.dmg` and replace the app by hand. The goal is the usual
macOS behaviour: the app checks for a newer release, shows what changed,
and installs it and restarts on request. macOS comes first; the other
platforms need an evaluation (see below) before anything is built for
them.

## Current state

- Releases are built, signed and notarized on a Mac with
  `just release-build <version>` and published with
  `just release-publish <version>` as a GitHub release `v<version>` with
  one asset, `Torchsnap.dmg`, and the CHANGELOG section as notes
  (`just/release.just`, README "Releasing", ADR 0048, 0049, 0050).
- A version with a pre-release part becomes a GitHub prerelease and is
  not marked latest.
- The tray menu (`src-tauri/src/platform/macos/tray.rs`) has Open
  Launcher, Settings..., Developer Tools... and Quit Torchsnap.
- Settings → General shows `Build: v<version> (<git hash>)` from the
  `build_info` command.
- `tauri-plugin-process` is registered (Rust, JS package,
  `process:default` capability), so `relaunch()` is available to the
  frontend.

## Tauri's updater plugin

`tauri-plugin-updater` (Tauri v2), from its documentation:

- `bundle.createUpdaterArtifacts: true` makes the macOS build also
  produce `<App>.app.tar.gz` and `<App>.app.tar.gz.sig`.
- The signature comes from `TAURI_SIGNING_PRIVATE_KEY` (key content or
  path) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, set in the environment
  of the build. A key pair is generated with `tauri signer generate`.
  The public key goes into `plugins.updater.pubkey` in
  `tauri.conf.json`, so it is compiled into the app.
- `plugins.updater.endpoints` lists URLs to query. They may contain
  `{{target}}`, `{{arch}}` and `{{current_version}}`. An endpoint either
  serves a static JSON (`version`, `notes`, `pub_date`, and per platform
  key such as `darwin-aarch64` a `url` and a `signature`) or answers
  dynamically with 200 and that JSON, or 204 when there is no update.
- A static `latest.json` attached to the GitHub release can be reached
  as `https://github.com/jakobwesthoff/torchsnap/releases/latest/download/latest.json`.
  "latest" skips prereleases, which matches how betas are published now.
- Updates are full downloads of the signed archive, not deltas.
- JS API: `check()` returns an update or nothing,
  `update.downloadAndInstall(onProgress)` installs it, and
  `relaunch()` from the process plugin restarts. The capability
  `updater:default` allows check, download and install.
- Other platforms: Linux only as AppImage, Windows as MSI or NSIS
  (`plugins.updater.windows.installMode`: `passive`, `basicUi`,
  `quiet`).

## Proposed work (macOS)

1. **Signing key.** Generate the updater key pair. Keep the private key
   and its password next to the other release secrets (the file named
   by `TORCHSNAP_RELEASE_ENV`), never in the repository or GitHub. Put
   the public key into `tauri.conf.json`. Losing the private key means
   installed apps can no longer verify updates, so it needs a backup
   plan.
2. **Release pipeline.** Enable `createUpdaterArtifacts`.
   `release-build` checks that the signing variables are set, verifies
   the `.app.tar.gz` and its signature, and stages them next to
   `Torchsnap.dmg` in `release.json`. `release-publish` writes
   `latest.json` (version, CHANGELOG section as notes, date,
   `darwin-aarch64` URL of the uploaded archive, signature) and uploads
   the archive, its signature and `latest.json` with the DMG.
3. **Backend.** Add `tauri-plugin-updater`, the endpoint above, and the
   `updater:default` capability for the window that shows the update UI.
4. **UI, following the macOS convention:**
   - a "Check for Updates..." item in the tray menu;
   - an automatic check after startup and then periodically;
   - when an update is found: a window with the new version, the
     release notes and **Install and Restart**, **Later** and
     **Skip This Version**;
   - "You're up to date" feedback for a manual check that finds
     nothing, and an error message when the check fails;
   - in Settings → General next to the build line: "Check for Updates"
     and a switch for automatic checks.
5. **Docs.** README "Releasing" (key setup, new artifacts), an ADR for
   the update channel and key handling, the user docs (installation page
   and Settings), CHANGELOG.

## Open questions

- **Notarization of the update archive.** Does the `.app` inside
  `.app.tar.gz` carry the stapled notarization ticket, that is, does
  Tauri create the archive after notarizing the app? Check with
  `xcrun stapler validate` on an extracted archive, and let
  `release-build` enforce it.
- **Gatekeeper after an in-app update.** The DMG route gets a quarantine
  flag and the "downloaded from the internet" prompt. What does macOS do
  with an app replaced by the updater, and does launch at login still
  work afterwards?
- **Write access.** Can the updater replace `/Applications/Torchsnap.app`
  for a user without admin rights, and what does the plugin do when it
  cannot?
- **Automatic checks and privacy.** On by default or asked on first
  launch? The check contacts GitHub on every run of the interval; the
  docs should say so.
- **Pre-releases.** Offer a beta channel (a second endpoint that follows
  prereleases), or keep betas as manual downloads?
- **Plugin or Sparkle.** Sparkle is the established macOS updater
  (appcast, EdDSA signatures, its own UI). The plugin fits the Tauri
  stack and the other platforms; Sparkle would need native integration.
  Decide before building.
- **Pending gadget changes.** "Install and Restart" also applies pending
  gadget installs and uninstalls. Say so in the update window when
  there are pending changes?

## Cross-platform evaluation (not started)

- **Linux.** The plugin only updates AppImages. Packages from `.deb`,
  `.rpm` or Flatpak are updated by their package manager, so an in-app
  install would be wrong there. One option: only notify and link to the
  download page for those formats. Depends on which formats Linux
  releases ship, which is not decided
  (`todos/platform/linux/`).
- **Windows.** MSI or NSIS with an `installMode`. Windows releases do
  not exist yet.
