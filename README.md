# Torchsnap

**Light. Find. Launch.**

Strike a key, light the way. A cross-platform, keyboard-driven launcher
with sandboxed WebAssembly Gadgets.

Torchsnap is a Tauri 2 desktop application (Rust host, React + Vite
frontend) with a sandboxed WebAssembly gadget system. Gadgets are
WASM components targeting `wasm32-wasip2`, distributed as
single-file `.torchsnap` archives. The host (`src-tauri/`) loads them
at runtime via `wasmtime`, mediates every host capability through a
manifest-declared permission model, and streams search results into
the launcher UI.

The primary supported platform today is macOS.

## Installing gadgets

Third-party gadgets ship as `.torchsnap` files, single-file archives
you can hand-share. Open one in Torchsnap to install it: double-click
it in Finder, drop it on **Settings → Gadgets** (or use **Choose a
file…** there), or pass its path to the binary on the command line.
Torchsnap then shows a review with the gadget's details and every
permission it declares. Nothing is installed until you confirm.

Installs, updates and uninstalls take effect after a restart. Until
then the Gadgets list shows each pending change with an **Undo**
button, and a bar above the list offers **Restart now**.

Installed user gadgets live under
`<app_data_dir>/gadgets/<id>.torchsnap` and their host-managed state
(SQLite databases, caches) under `<app_data_dir>/gadget-home/<id>/`.
Installing another version of a gadget replaces the archive and keeps
that state. Uninstalling deletes both on the next start.

Gadgets bundled with the app (like the calculator) cannot be
uninstalled. They are upgraded along with Torchsnap itself.

[Installing Gadgets](https://docs.torchsnap.app/start/gadgets/installing/)
in the user documentation covers the review, updates and uninstalling
in detail.

## Developing gadgets

See [`docs/api/gadget-development.md`](docs/api/gadget-development.md)
for the gadget author guide: the WIT interface, manifest format,
discovery rules, and packaging instructions.

For the host-side architecture (component model, lifecycle, search
dispatch, host capabilities, frontend integration), start with
[`docs/Gadget-Architecture/01-overview.md`](docs/Gadget-Architecture/01-overview.md).
Design rationale for individual subsystems lives in
[`docs/adr/`](docs/adr/).

## Setting up a fresh checkout

1. Install [`just`](https://github.com/casey/just), then run
   `just doctor`. It lists every external tool the recipes need
   (rustup, bun, zip, wasm-tools, ImageMagick, cwebp, jq, curl) and
   how to install the missing ones.
2. Run `just install`. It generates the gitignored assets the build
   reads (app and tray icons, mascot data, timezone data, and the
   DuckDuckGo bang database, which it downloads), runs `bun install`
   for the host and every gadget frontend, and fetches the crates of
   `src-tauri/` and `gadgets/`.
3. Run `just fullcycle` to confirm the checkout passes every quality
   gate.
4. Run `just start` to launch the app in development mode.

`rust-toolchain.toml` pins the Rust release together with the
`wasm32-wasip2` target, clippy and rustfmt. rustup installs them the
first time `cargo` runs in the repository.

No gadget has to be staged before the host compiles. `src-tauri/build.rs`
creates an empty `target/bundled-gadgets/` if needed, and
`just build` stages the bundled gadgets itself.

## Building from source

Torchsnap uses [`just`](https://github.com/casey/just) as its task
runner and [`bun`](https://bun.sh) for the frontend. Gadgets are
built with plain `cargo build --release` against the
`wasm32-wasip2` target — `cargo-component` is **not** used. WIT
inspection and formatting use `wasm-tools`.

Common recipes (run `just --list` for the full set):

- `just build --release` — release build. Stages whitelisted
  gadgets from `gadgets/bundled.toml` into `target/bundled-gadgets/`,
  builds every in-tree gadget, then runs `tauri build`.
- `just build` — the same flow as a debug build
  (`tauri build --debug`). Debug is the default.
- `just build --release --sign` — release build with a code-signed
  macOS bundle; see [Signing macOS builds](#signing-macos-builds).
- `just build-gadget <name>` — rebuild a single gadget under
  `gadgets/<name>/` and repackage it as `<name>.torchsnap`.
- `just check-gadgets` / `just check-wit` — fast workspace and WIT
  validation.

The repo-root `target/` directory is owned by the bundled-gadget
staging flow and is gitignored. Cargo's own build outputs go to
`src-tauri/target/` (host) and `gadgets/target/` (gadget
workspace).

## Signing macOS builds

`just build` leaves the macOS bundle unsigned unless the environment
sets `APPLE_SIGNING_IDENTITY`. `--sign` sets that variable to `-` (ad-hoc)
when it is not already set, and keeps any identity it finds there. Tauri
then signs the bundle with the hardened runtime and the entitlements in
`src-tauri/Entitlements.plist`, which gadget code needs to run under the
hardened runtime (ADR 0046).

Build anything meant for other people with `--sign`. An unsigned bundle
fails `syspolicy_check distribution` with the fatal error "Code has no
resources but signature indicates they must be present", and macOS
reports a downloaded copy as damaged. Keep local builds unsigned when you
want to attach lldb to the bundled app: lldb cannot attach to a signed
bundle (ADR 0047).

The build writes `src-tauri/target/release/bundle/macos/Torchsnap.app` and
`src-tauri/target/release/bundle/dmg/Torchsnap_<version>_<arch>.dmg`.

### Ad-hoc signing

```sh
just build --release --sign
```

No Apple account is involved. Users who download the DMG in a browser
still see a Gatekeeper warning on first launch and approve the app once
under System Settings → Privacy & Security → "Open Anyway", which asks
for an administrator password.

### Developer ID signing and notarization

Notarization needs a paid Apple Developer account and a
"Developer ID Application" certificate. With the variables below set,
`just build --release --sign` signs with the Developer ID instead of
ad-hoc, and Tauri notarizes the app and staples the ticket to it. Tauri
only signs the DMG, so the build then runs `just notarize-dmg`, which
notarizes the DMG and staples its ticket as well.

Signing:

| Variable | Value |
| --- | --- |
| `APPLE_SIGNING_IDENTITY` | The certificate's keychain identity, e.g. `Developer ID Application: <Name> (<Team ID>)` |
| `APPLE_CERTIFICATE` | Base64-encoded `.p12` export of the certificate. Only needed where the certificate is not in the keychain, such as CI. |
| `APPLE_CERTIFICATE_PASSWORD` | Password of that `.p12` export |

Notarization, with an App Store Connect API key:

| Variable | Value |
| --- | --- |
| `APPLE_API_ISSUER` | Issuer ID of the key |
| `APPLE_API_KEY` | Key ID |
| `APPLE_API_KEY_PATH` | Path to the downloaded `.p8` key file |

Or with an Apple ID: `APPLE_ID`, `APPLE_PASSWORD` (an app-specific
password) and `APPLE_TEAM_ID`.

### Checking a build

```sh
codesign -dv --entitlements - src-tauri/target/release/bundle/macos/Torchsnap.app
syspolicy_check distribution src-tauri/target/release/bundle/macos/Torchsnap.app
```

A signed build shows `flags=0x10002(adhoc,runtime)` for ad-hoc or
`flags=0x10000(runtime)` for Developer ID, plus the two entitlements.
`syspolicy_check` reports only a warning for an ad-hoc build. For a
notarized build, `spctl -a -vv -t exec <app>` reports
`source=Notarized Developer ID`, and so does
`spctl -a -vv -t open --context context:primary-signature <dmg>` for its
DMG. `xcrun stapler validate <app or dmg>` confirms the stapled ticket.

## Releasing

Releases are built, notarized and published from a Mac, not from CI, so
no signing credentials are stored in GitHub (ADR 0050). Each release
carries one Apple silicon DMG named `Torchsnap.dmg` (ADR 0048).

### One-time setup

- The "Developer ID Application" certificate with its private key in the
  login keychain.
- An App Store Connect API key (`.p8` file, Key ID, Issuer ID).
- `~/.config/torchsnap/release.env`, or another file named by
  `TORCHSNAP_RELEASE_ENV`:

  ```sh
  APPLE_SIGNING_IDENTITY="Developer ID Application: <Name> (<Team ID>)"
  APPLE_API_ISSUER="<Issuer ID>"
  APPLE_API_KEY="<Key ID>"
  APPLE_API_KEY_PATH="$HOME/.appstoreconnect/private_keys/AuthKey_<Key ID>.p8"
  ```

- The GitHub CLI `gh`, logged in with push access to this repository.

### Making a release

1. Set the new version in `package.json`, `src-tauri/Cargo.toml` and
   `src-tauri/tauri.conf.json`.
2. In `CHANGELOG.md`, turn `[Unreleased]` into `## [<version>] - <today>`,
   start a new empty `[Unreleased]` section, and add the link definition
   `[<version>]: https://github.com/jakobwesthoff/torchsnap/releases/tag/v<version>`
   at the end of the file.
3. Commit and push to `main`.
4. `just release-build <version>` checks all of the above, runs
   `just install` and `just fullcycle`, builds, signs and notarizes the
   app and the DMG, verifies them, and stages
   `src-tauri/target/release/dist/Torchsnap.dmg`. Try that DMG.
5. `just release-publish <version>` tags `v<version>`, pushes the tag and
   creates the GitHub release "Torchsnap <version>" with `Torchsnap.dmg`.
   The notes are the CHANGELOG section plus the installation link and the
   DMG's SHA-256.

A version with a pre-release part, such as `0.10.0-beta.1`, becomes a
GitHub prerelease and is not marked latest, so the download link on
torchsnap.app keeps serving the last stable release.

If the build must change, fix it on `main` and run `release-build` again:
`release-publish` only publishes the DMG staged for the current commit.

## License

Mozilla Public License Version 2.0 (MPL-2.0). Every source file
carries the standard MPL header — see `CLAUDE.md` for the per-
language header formats.
