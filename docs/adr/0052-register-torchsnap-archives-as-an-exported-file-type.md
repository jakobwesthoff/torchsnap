# 52. Register torchsnap archives as an exported file type

Date: 2026-09-24

## Status

Accepted

## Context

Opening a `.torchsnap` archive from Finder hands it to the install queue
of ADR 51, but only if the bundle tells macOS that Torchsnap opens that
file type. Tauri's `bundle.fileAssociations` generates the
`CFBundleDocumentTypes` and `UTExportedTypeDeclarations` keys, and the
same configuration feeds the `MimeType=` line of the Linux `.desktop`
file. It has no field for a document icon. The bundler merges
`src-tauri/Info.plist` into the generated plist, and a key present in
that file replaces the generated key as a whole.

## Decision

`.torchsnap` is registered with these values:

| Key | Value |
|---|---|
| Exported type identifier (UTI) | `app.torchsnap.gadget` |
| Conforms to | `public.data` only |
| MIME type | `application/vnd.torchsnap.gadget+zip` |
| Extension | `torchsnap` |
| Type name | `Torchsnap Gadget` |
| `CFBundleTypeRole` | `Viewer` |
| `LSHandlerRank` | `Owner` |
| Document icon | `gadget-document.icns`, a page carrying the app icon |

The type does not declare zip conformance on either platform, although
the MIME type keeps the `+zip` suffix.

Both keys are written in full in `src-tauri/Info.plist`, with
`UTTypeIconFile` and `CFBundleTypeIconFile` naming the document icon.
`bundle.fileAssociations` in `tauri.conf.json` carries the same values.
The icon is generated from `src-tauri/icons/app-icon-source.png` by
`just asset-document-icon` and bundled into `Contents/Resources` through
`bundle.resources`.

`tools/verify-bundle-plist`, run by `just verify-bundle` and by
`just release-build`, checks every value above in a built bundle.

## Consequences

- The type exists only in bundled builds. `tauri dev` runs a bare
  binary that LaunchServices does not register.
- The registration values are defined twice, in `src-tauri/Info.plist`
  and in `tauri.conf.json`.
