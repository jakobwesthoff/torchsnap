---
kind: feature
status: open
tags: [macos]
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
depends-on: [todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md]
---

# macOS: register `.torchsnap` with Torchsnap and receive opened files

First implementation target, after the intake pipeline. Plan steps 15
and 17.

## Goal

Double-clicking a `.torchsnap` file in Finder, or choosing "Open With
→ Torchsnap", starts Torchsnap if needed and hands the file to the
install intake
(`todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`).

## Decisions (2026-09-24)

| Key | Value |
|---|---|
| UTI | `app.torchsnap.gadget` |
| Conforms to | `public.data` only (no `public.zip-archive`) |
| MIME type | `application/vnd.torchsnap.gadget+zip` |
| Extension | `torchsnap` |
| `CFBundleTypeName` | `Torchsnap Gadget` |
| `CFBundleTypeRole` | `Viewer` |
| `LSHandlerRank` | `Owner` |
| Document icon | Yes, in the first version, derived from the app icon |

## Registration

- `bundle.fileAssociations` in `src-tauri/tauri.conf.json` with the
  values above. Tauri's `file_associations_plist()`
  (`tauri-utils-2.9.3/src/config.rs:1302-1409`) turns it into
  `CFBundleDocumentTypes` and `UTExportedTypeDeclarations`. It has no
  icon field.
- The bundler merges `src-tauri/Info.plist` last and replaces whole
  top-level keys. So `src-tauri/Info.plist` carries complete
  `CFBundleDocumentTypes` and `UTExportedTypeDeclarations` entries
  with `CFBundleTypeIconFile` and `UTTypeIconFiles` added. The
  `fileAssociations` entry stays for Linux (`MimeType=` in the
  `.desktop` file) and must hold the same values.
- `src-tauri/icons/gadget-document.icns` is generated from
  `app-icon-source.png` by a `just` recipe and placed in
  `Contents/Resources` through `bundle.resources`.
- `just verify-bundle-plist` reads the built bundle's `Info.plist`
  with `plutil -extract` and fails if any value above is missing or
  different. It runs after `just build` and in the release recipe.

## Receiving files

`RunEvent::Opened { urls }` is compiled only for macOS, iOS and
Android. A new arm in the `app.run` closure passes the URLs through
`intake::paths_from_opened_urls` (keeps `file://`, logs others),
submits them with origin `OsOpenFile`, and shows the settings window.

tao forwards `application:openURLs:` only when its callback is
installed and has no queue. The intake queue is created in `run()`
before the builder and shared with the run-loop closure, so the order
of `Opened` and `setup` on a cold launch does not matter.

## Platform notes

- **No Dock icon.** The activation policy is `Accessory`, so there is
  nothing to drop files on. Finder double-click and "Open With" work.
- **Not sandboxed.** `src-tauri/Entitlements.plist` only holds the
  hardened-runtime exceptions from ADR 0046, so reading paths and
  xattrs from open events needs no security-scoped bookmarks.
- **Dev builds register nothing.** `tauri dev` embeds only
  `CFBundleName` and the version keys of `src-tauri/Info.plist` into
  the binary. Associations only exist for the bundled app.

## Testing

Automated: the URL filter (unit tests) and `just verify-bundle-plist`.
Manual: the checklist in the plan (cold start, running app, several
files, quarantined download, replace, notarized DMG). Useful commands:

- `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -dump | rg -i torchsnap`
- `lsregister -f /Applications/Torchsnap.app` after plist changes
- `mdls -name kMDItemContentType some.torchsnap`
