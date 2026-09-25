# Torchsnap: project rules

## Workflow

- Run `just fullcycle` before committing code changes.
- Every user-visible change gets a `CHANGELOG.md` entry under
  `[Unreleased]` (Keep a Changelog: `Added`, `Changed`, `Removed`,
  `Fixed`).

## Tests and quality gates

- At the start of a session, before the first change that can affect
  the gates' outcome, run `just fullcycle` once to get a baseline. If
  the baseline already fails, tell the user what fails and propose
  fixing it before starting the other work.
- Every piece of code you touch gets thorough test coverage, edited code
  as much as new code. Cover the behavior of each changed path,
  including its error and edge cases, not just the happy path.
- Work test-first. For a bug, write a regression test that reproduces
  it, run it and see it fail, then fix the code until it passes.
- Keep every quality gate green at all times: `just fmt-check`,
  `just lint`, `just check`, `just test`. Clippy runs with
  `-D warnings`, so a clippy warning is a failure. Fix it rather than
  silencing it with `#[allow]`.

## License headers

Every source file starts with the MPL-2.0 header, after a shebang line if
there is one. Add it to any source file that lacks it.

Rust, TypeScript, JavaScript:
```
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
```

CSS:
```
/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
```

HTML:
```
<!-- This Source Code Form is subject to the terms of the Mozilla Public
   - License, v. 2.0. If a copy of the MPL was not distributed with this
   - file, You can obtain one at https://mozilla.org/MPL/2.0/. -->
```

Shell scripts, `Justfile`, `.just`:
```
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
```

SQL:
```
-- This Source Code Form is subject to the terms of the Mozilla Public
-- License, v. 2.0. If a copy of the MPL was not distributed with this
-- file, You can obtain one at https://mozilla.org/MPL/2.0/.
```

WIT: same text with `///` prefix.

No header: `.json`, `.toml`, `.lock`, `.md`, images, other non-source
config.

`.devcontainer/` is not MPL-licensed (adapted third-party files, see
`.devcontainer/NOTICE.md`). Never add MPL headers there.

## Gadgets

- Build with plain `cargo build --release` in `gadgets/`;
  `gadgets/.cargo/config.toml` sets the default target `wasm32-wasip2`.
  `cargo-component` is not used; do not add it or recipes that need it.
- `wit_bindgen::generate!` lives in `gadgets/gadget-sdk/`. Gadgets use
  `use torchsnap_gadget_sdk::prelude::*;` and `define_gadget!(MyGadget)`.
- Only ids listed in `gadgets/bundled.toml` ship in the app. Unlisted
  gadgets under `gadgets/<id>/` still load in debug builds (ADR 0035).
- SQLite files use the `.sqlite3` extension (ADR 0018).

## Mascots

- New or changed Snappy images follow
  `assets/mascot/docs/Adding-a-Mascot.md`: 1024×1024 source, body size
  proposed with `tools/normalize-mascot-size --output-dir` (never
  enlarged), `oxipng -o max --strip safe`, and only images whose pixels
  changed committed.
- Before any mascot source is resized, show the maintainer a sheet from
  `tools/mascot-size-sheet` with references, the current and the
  proposed version, and apply only what they approve.

## ADRs

Record decisions in `docs/adr/`:

- Create: `EDITOR=true adrs new "<title>"`, then fill Context, Decision,
  Consequences and set Status to `Accepted`.
- Link changed ADRs both ways, below the status: `Amends [N. Title](file)`
  in the new one, `Amended by [N. Title](file)` in the old one.
- Record only what was decided; no invented rationale.

## Releases and signing

- `just build --release --sign`: signs with `APPLE_SIGNING_IDENTITY`
  (default `-`, ad-hoc), the hardened runtime and the entitlements in
  `src-tauri/Entitlements.plist` (ADR 0046, 0047). Keep local builds
  unsigned when lldb must attach.
- With a Developer ID and notarization credentials in the environment,
  Tauri notarizes the app and `just notarize-dmg` (run by `--sign`)
  notarizes the DMG (ADR 0049). Variables and checks: README, "Signing
  macOS builds".
- Each release ships one arm64 DMG uploaded as `Torchsnap.dmg`; the
  bundle is `Torchsnap.app`, identifier `app.torchsnap` (ADR 0048).
- Releases are made locally with `just release-build <version>` and
  `just release-publish <version>`, never from CI; no signing secrets in
  GitHub (ADR 0050, README "Releasing").
- Next to the DMG, a release uploads the update archive
  `Torchsnap.app.tar.gz`, its `.sig` and `release.json`, the update
  feed written by `tools/release-feed`. torchsnap.app serves the latest
  `release.json` as `/updates/latest.json` (ADR 0053).
- Release order (README "Making a release"): version bump and CHANGELOG
  section committed and pushed, torchsnap-docs pushed with it,
  `release-build`, the maintainer tries the DMG, `release-publish`, then
  check `https://torchsnap.app/updates/latest.json`.
- From `release-build` until `release-publish` has finished, change
  nothing in the main checkout: `release-build` refuses a build that
  changed tracked files, and `release-publish` refuses when `HEAD`
  moved. Prepare other changes in a worktree and merge them afterwards.
- Run `release-build` with the sandbox disabled; notarization and code
  signing timestamps need Apple's servers.
- Never print or read the updater key or its password
  (`TORCHSNAP_UPDATER_KEY_PATH`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in
  `release.env`); scripts load them into the environment without
  echoing them.
- Everyday builds make no update archive. `just build --config <json>`
  passes `bundle.createUpdaterArtifacts` (and test-only settings such as
  `plugins.updater.dangerousInsecureTransportProtocol`) to `tauri build`.
  `TORCHSNAP_UPDATE_FEED` points an installed build at a test feed;
  start the app with `open --env TORCHSNAP_UPDATE_FEED=<url>`, since
  `launchctl setenv` does not reach apps opened from Finder.
