# Torchsnap: project rules

## Workflow

- Run `just fullcycle` before committing code changes.
- Every user-visible change gets a `CHANGELOG.md` entry under
  `[Unreleased]` (Keep a Changelog: `Added`, `Changed`, `Removed`,
  `Fixed`).

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
