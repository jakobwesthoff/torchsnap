# 53. Deliver updates through a signed feed on torchsnap.app

Date: 2026-09-24

## Status

Accepted

Amends [50. Build and publish releases locally](0050-build-and-publish-releases-locally.md)

## Context

Torchsnap cannot update itself. A user replaces the app by hand with a
new `Torchsnap.dmg` from the GitHub release (ADR 0048, 0050).

`tauri-plugin-updater` (2.12) reads a JSON feed from a configured URL.
The feed names a version and, per platform (`darwin-aarch64` on Apple
silicon), the URL of a `.app.tar.gz` and its minisign signature. The
plugin offers the update when the version is higher than the installed
one, verifies the downloaded archive against the public key in
`plugins.updater.pubkey` and replaces the running `.app`. The feed
itself is not signed. With `requireSignedVersion` the plugin also
rejects an archive whose signature was made for a different version
than the feed announces. Release builds accept only https feed URLs.

`tauri build` produces the archive and its `.sig` when
`bundle.createUpdaterArtifacts` is set, and then requires the private
key in `TAURI_SIGNING_PRIVATE_KEY`. An updater spike on 2026-09-24
with signed, notarized builds showed:

- `tauri build` notarizes and staples the `.app` before it packs the
  archive, so the archived app carries the ticket.
- The `.sig` records the version in its trusted comment.
- An app installed by drag and drop into `/Applications` was replaced
  without a password prompt and without a Gatekeeper prompt, carried no
  quarantine attribute afterwards and still started at login.
- The CLI ignored `TAURI_SIGNING_PRIVATE_KEY_PATH`; the key content in
  `TAURI_SIGNING_PRIVATE_KEY` works.

## Decision

Updates use `tauri-plugin-updater`. The app reads its feed from
`https://torchsnap.app/updates/latest.json` and sets
`requireSignedVersion: true`.

The feed of a release is a file `release.json`:

- `version`, `pub_date` (RFC 3339) and `notes` (the release's CHANGELOG
  section);
- `platforms.darwin-aarch64` with `url` pointing at
  `Torchsnap.app.tar.gz` of exactly that release
  (`releases/download/v<version>/…`, not `latest/download`) and
  `signature` (the content of the `.sig`);
- `releases`: version, date and notes of every CHANGELOG section that is
  not a pre-release;
- commit and DMG SHA-256, as in the staging file `release-build` writes
  today.

This amends ADR 0050:

- `release-build` builds with `createUpdaterArtifacts`, checks the
  archive (stapled ticket, signature, `version:<version>` in the trusted
  comment) and writes the full `release.json` in place of today's
  staging file.
- `release-publish` checks `release.json` as before and uploads
  `Torchsnap.dmg`, `Torchsnap.app.tar.gz`, `Torchsnap.app.tar.gz.sig`
  and `release.json`. It then starts the torchsnap-web deploy with
  `gh workflow run deploy.yml -R jakobwesthoff/torchsnap-web`.
- The torchsnap-web build downloads `release.json` of the latest release
  from GitHub and serves it as `/updates/latest.json`. The build fails
  when the download fails.

Pre-releases are never offered by the updater.

The updater key pair is made with `tauri signer generate`. The private
key lives in `~/.config/torchsnap/updater.key`; `release.env` names it in
`TORCHSNAP_UPDATER_KEY_PATH` and holds its password in
`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. Key and password are also kept in
the maintainer's password manager. The public key is in
`src-tauri/tauri.conf.json`.

The environment variable `TORCHSNAP_UPDATE_FEED` replaces the feed URL
in every build, for testing against another feed.

Rejected: Sparkle through the third-party
`tauri-plugin-sparkle-updater`.

## Consequences

- Only a holder of the private key can produce an archive the installed
  apps accept. Losing the key or its password means installed apps
  cannot verify any further update and have to be reinstalled by hand
  from a DMG built with a new key.
- `release.env` is exported into every process of `release-build`,
  including `bun` and cargo build scripts, so they see the key and its
  password, as they already see the Apple credentials.
- Everyday builds do not set `createUpdaterArtifacts` and need no key.
- Every update check sends a request to torchsnap.app, served by
  GitHub Pages, which sees the IP address.
- A release is only offered to installed apps after the torchsnap-web
  deploy has run.
- Users of 0.11.x and earlier have no updater and install the first
  version with one by hand.
