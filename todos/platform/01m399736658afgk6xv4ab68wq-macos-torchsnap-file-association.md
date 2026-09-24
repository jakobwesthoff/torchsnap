---
kind: feature
status: open
tags: [macos]
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
depends-on: [todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md]
---

# macOS: register `.torchsnap` with Torchsnap and receive opened files

First implementation target. Needs the intake design settled first.

## Goal

Double-clicking a `.torchsnap` file in Finder, or choosing "Open With
→ Torchsnap", starts Torchsnap if needed and hands the file to the
install intake
(`todos/gadget-host/install/01m399736658afgk6xv4ab68wm-install-request-intake.md`).

## Registration

Tauri 2 (`tauri` 2.11.6, `tauri-utils` 2.9.3 in `Cargo.lock`) supports
this through `bundle.fileAssociations` in `src-tauri/tauri.conf.json`.
From the `FileAssociation` struct in
`tauri-utils-2.9.3/src/config.rs:1185-1256`:

| Field | macOS mapping |
|---|---|
| `ext` | file extensions |
| `name` | `CFBundleTypeName`, defaults to `ext[0]` |
| `role` | `CFBundleTypeRole` |
| `rank` | `LSHandlerRank` |
| `mimeType` | `public.mime-type` in the exported type's tag spec (also Linux `MimeType=`) |
| `exportedType.identifier` | `UTTypeIdentifier` of a `UTExportedTypeDeclarations` entry |
| `exportedType.conformsTo` | `UTTypeConformsTo` |
| `contentTypes` | `LSItemContentTypes` |

Starting point, values still open:

```json
"fileAssociations": [
  {
    "ext": ["torchsnap"],
    "name": "Torchsnap Gadget",
    "role": "Viewer",
    "rank": "Owner",
    "mimeType": "application/x-torchsnap-gadget",
    "exportedType": {
      "identifier": "app.torchsnap.gadget",
      "conformsTo": ["public.data"]
    }
  }
]
```

`FileAssociation` has no icon field. A document icon for `.torchsnap`
files (`UTTypeIconFile` / `CFBundleTypeIconFile`) would need extra
Info.plist keys. My understanding is that Tauri 2 merges a
`src-tauri/Info.plist` into the generated one; to verify. The icon
itself needs design work
(`todos/product/visual-identity/01kmh1h2b95jvtfk76hwyd80ke-design-logo-and-icons.md`).
Optional for the first version.

## Receiving files

`RunEvent::Opened { urls: Vec<url::Url> }` is only compiled for macOS,
iOS and Android (`tauri-2.11.6/src/app.rs:263`). Add an arm to the
`app.run` closure (`src-tauri/src/lib.rs:990-1016`) that turns
`file://` URLs into paths and passes them to the intake with origin
`OsOpenFile`. Any other scheme gets logged and ignored until the URL
scheme todo claims it; the deep-link plugin receives its URLs through
this same event on macOS.

## Platform notes

- **No Dock icon.** The activation policy is `Accessory`
  (`lib.rs:932-935`), so there is nothing to drop files on. Finder
  double-click and "Open With" still work, since LaunchServices sends
  the open event to the running process either way.
- **Not sandboxed.** `src-tauri/Entitlements.plist` only holds the
  hardened-runtime exceptions from ADR 0046, so reading an arbitrary
  path from an open event needs no security-scoped bookmark. That
  changes if the app is ever sandboxed.
- **Quarantined downloads.** Files downloaded by a browser carry
  `com.apple.quarantine`. Opening a document does not trigger
  Gatekeeper, and the review dialog could use the quarantine and
  `kMDItemWhereFroms` data (see
  `todos/gadget-host/install/01m399736658afgk6xv4ab68wn-install-review-dialog.md`).
- **Cold start timing.** When LaunchServices starts the app for a
  file, it is not yet verified whether `Opened` fires before or after
  `setup`. The intake must handle both; test this explicitly.

## Open points

- UTI and MIME names, and whether to conform to `public.zip-archive`.
  Shared with Linux, so decided in the plan.
- `role` `Viewer` or `None`? Torchsnap neither edits nor really views
  the file. `Viewer` is the usual choice for "opens it". Check what
  Finder shows in "Open With" and Get Info for each.
- `rank` `Owner` claims the type as ours. Since we export the UTI,
  that fits.

## Testing

Associations only exist for a bundled app. `just start` runs the
unbundled dev binary and registers nothing.

- Build with `just build --release` (optionally `--sign`), copy
  `Torchsnap.app` into `/Applications`, launch it once.
- Check registration:
  `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -dump | rg -i torchsnap`
  and `mdls -name kMDItemContentType some.torchsnap`. Force a
  re-register with `lsregister -f /Applications/Torchsnap.app` after
  changing the plist.
- Cases: app not running; app running with settings closed; settings
  open on another section; several files selected and opened at once;
  a file from Downloads with the quarantine flag; a non-gadget file
  renamed to `.torchsnap`.
- Check the notarized release DMG as well, since `release-build`
  signs and notarizes (ADR 0049, 0050). The added plist keys should
  not affect that, but verify once.

## Also on completion

- ADR for the association and the type identifiers.
- `torchsnap-docs`: `start/settings.mdx:94` and
  `development/packaging.mdx:132-163` (see the plan).
- `CHANGELOG.md` `Added` entry.
