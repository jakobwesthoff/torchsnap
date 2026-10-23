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

## Todos

- `todos/` is an ntropy vault; its `README.md` lists the components and
  tags. File, update and close todos through the `ntropy-todos` skill,
  never by writing files into `todos/all-notes/` by hand. Code comments
  and docs cite a todo by its current filename without a path
  (`<ULID>-<slug>.md`); a retitle renames the file, so update those
  citations with it.
- Once a todo is implemented, delete it and deal with every reference
  to it (find them with
  `rg -i <ulid> . ../torchsnap-docs ../torchsnap-web`). If a
  referencing file needs information from the todo, copy just that part
  into it, compact and precise.
- After a large work package is done, have Sonnet or Haiku agents
  (choose per scan) check all todos for work that is already done.
  Delete todos that are fully done and rewrite partly done ones so they
  cover only the remaining work.

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

## Linux support

- Linux support is built one desktop environment at a time, in this
  order: GNOME, labwc, X11, KDE. On each, first how the app looks, how
  it is invoked and how it renders, then app discovery, settings
  discovery and the remaining features; then the next desktop. Refactor
  when a later desktop needs it. Once all are done, test all of them
  again.
- When work on one desktop finds something another Linux desktop needs
  in order to work, file a todo for it right away, naming the desktops
  it affects.
- Run and test Torchsnap on Linux through the testbed: the
  `linux-testbed` skill and `docs/Howto-test-on-a-raspberry-pi.md`
  (`just testbed-*` recipes: sync, build, start, control, screenshots,
  input, self-test). Build and lint for Linux arm64 on the Mac in a
  container: `docs/Howto-build-linux-on-macos.md` (`just linux-build*`).
- Prefer the control API (`docs/control-api.md`: `status`, `windows`,
  `info`, `snapshot`) over screenshots when checking what the app does.

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

- The mascot set is made in the torchsnap-mascot repository. Its export
  writes `public/images/mascot/*.webp`, `src/derived/mascots.json` and
  `src/derived/mascot-groups.json` here; never edit them by hand. New or
  changed mascots, groups, occasions, sizes, alt texts and ground anchors
  are changed there and exported again.
- `src/mascot/variants.ts` builds the pools from each mascot's `group`
  and `occasions`, the groups' `seasonal` flag and
  `src/mascot/occasions.json` (order, condition name and share per
  occasion); `src/mascot/selection.ts` draws a character, then one of
  its variants, by each mascot's `character` (ADR 58, 59). A new
  seasonal group needs an entry in `occasions.json`, a new kind of
  timing a condition in the registry `src/mascot/conditions.ts`.
  `src/launcher/placement.ts` places a mascot by its `groundAnchor`.

## App icon

- The app icon is a candidate of `assets/app-icon/` (README there),
  named by `app_icon` in `just/assets.just` (ADR 60). After changing the
  candidate or its document, run `just app-icon-compile` (needs Xcode
  with Icon Composer) and commit `assets/app-icon/compiled/`; never edit
  those files or the candidates' generated files by hand.

## Asset files

- Outputs of tools whose version the repository pins (`bun.lock`,
  `Cargo.lock`) are built, not tracked; outputs of unpinned tools (macOS
  built-ins, Xcode, Homebrew tools, image models) are tracked with their
  sources (ADR 61). A guideline, decided per case.
- The DMG background is `assets/dmg-background/` (README there). After
  changing its picture or scene, run `just dmg-background` and commit
  the files it writes. The repository has no script that calls an image
  model.
- Provenance in tracked files carries dates only, no times of day.

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
  section committed and pushed, `release-build`, the maintainer tries
  the DMG, `release-publish`, torchsnap-docs `main` fast-forwarded to
  `next` and pushed (not for a pre-release), then check
  `https://torchsnap.app/updates/latest.json`.
- Docs changes for unreleased work go to torchsnap-docs' `next` branch,
  never its `main` (torchsnap-docs ADR 0003).
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
