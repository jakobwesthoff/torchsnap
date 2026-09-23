# 49. Notarize release builds with an App Store Connect API key

Date: 2026-09-23

## Status

Accepted

Amends [47. Sign macOS bundles on request with --sign](0047-sign-macos-bundles-on-request-with-sign.md)

## Context

ADR 0047 left Developer ID signing and notarization to environment
variables that Tauri reads, without having run them. A paid Apple
Developer account now exists, with a "Developer ID Application"
certificate for team `B4K735S286`.

Tauri notarizes with either an App Store Connect API key
(`APPLE_API_ISSUER`, `APPLE_API_KEY`, `APPLE_API_KEY_PATH`) or an Apple
ID with an app-specific password (`APPLE_ID`, `APPLE_PASSWORD`,
`APPLE_TEAM_ID`).

The first notarized build (0.9.3, 2026-09-23) showed:

- Apple's notary service accepted the app with the hardened runtime and
  both entitlements from ADR 0046.
- Tauri notarizes the app and staples its ticket, but only signs the
  DMG. `spctl -a -t open --context context:primary-signature` rejected
  that DMG with `source=Unnotarized Developer ID`.
- After the DMG was notarized and stapled as well, `spctl` accepted the
  DMG and the app as `source=Notarized Developer ID`. Downloaded in a
  browser, the app showed only macOS's confirmation for apps downloaded
  from the internet ("Apple checked it for malicious software and none
  was detected"), with Open as the default button.

## Decision

Release builds are signed with the Developer ID Application certificate
and notarized with App Store Connect API keys. Local builds and CI use
separate keys, so either can be revoked on its own.

`just build --sign` runs `just notarize-dmg` after `tauri build`. The
recipe submits the DMG of the configured product name and version with
`notarytool`, requires the status `Accepted`, and staples the ticket. It
skips ad-hoc signed builds and builds without notarization credentials.
It accepts the Apple ID variables as well as the API key.

## Consequences

- A signed build with credentials makes two notary submissions, one for
  the app and one for the DMG, and needs network access to Apple.
- Downloaders no longer need System Settings or an administrator
  password to start the app.
- The README section "Signing macOS builds" describes the DMG step and
  how to check both artifacts.
