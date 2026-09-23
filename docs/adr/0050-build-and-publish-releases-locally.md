# 50. Build and publish releases locally

Date: 2026-09-23

## Status

Accepted

## Context

A release DMG has to be signed with the Developer ID certificate and
notarized with an App Store Connect API key (ADR 0049), and it is
uploaded as `Torchsnap.dmg` (ADR 0048). A tag-triggered GitHub Actions
workflow would need the certificate, its password and the API key as
repository secrets. The maintainer does not want signing keys or tokens
stored in GitHub.

## Decision

Releases are built, notarized and published from the maintainer's Mac
with two recipes in `just/release.just`:

- `just release-build <version>` checks that `main` is checked out,
  clean and equal to `origin/main`, that tag `v<version>` does not exist,
  that `package.json`, `src-tauri/Cargo.toml` and
  `src-tauri/tauri.conf.json` carry `<version>`, and that `CHANGELOG.md`
  has the section `## [<version>] - <today>` and its link definition. It
  loads the signing and notarization settings from
  `~/.config/torchsnap/release.env` (overridable with
  `TORCHSNAP_RELEASE_ENV`), runs `just install`, `just fullcycle` and
  `just build --release --sign`, verifies notarization, stapled tickets,
  `arm64` and the bundle version, and stages `Torchsnap.dmg` with a
  `release.json` (version, commit, SHA-256) in
  `src-tauri/target/release/dist/`.
- `just release-publish <version>` refuses unless the staged
  `release.json` matches the version, `HEAD` and the DMG's checksum and
  the git checks still hold. It creates the annotated tag `v<version>`
  ("Torchsnap <version>"), pushes it, and creates the GitHub release
  "Torchsnap <version>" with `Torchsnap.dmg` and notes made of the
  CHANGELOG section, a link to the installation guide and the SHA-256.
  A version with a pre-release part becomes a GitHub prerelease that is
  not marked latest; any other version is marked latest. It then checks
  the release's assets and, for a public repository, the download URL.

The version bump and the CHANGELOG section are prepared and committed by
hand before `release-build`.

GitHub Actions runs `ci.yml` (`just fullcycle`) without secrets.

Rejected: a tag-triggered release workflow with the signing secrets in a
GitHub environment.

## Consequences

- No signing certificate, password or API key is stored in GitHub.
- Only a Mac with the Developer ID certificate in its keychain and the
  notarization key can produce a release.
- A release is built from a commit that is already on `origin/main`.
- The CI API key created for GitHub Actions is not used.
