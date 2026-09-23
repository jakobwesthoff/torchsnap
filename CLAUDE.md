# Torchsnap: project rules

## Layout

- `src-tauri/`: Rust host (Tauri 2). Cargo target dir `src-tauri/target/`.
- `src/`: host frontend (React, Vite, TypeScript).
- `gadgets/`: WASM gadgets, one crate per `gadgets/<id>/`, plus
  `gadgets/gadget-sdk/` (Rust SDK, WIT in `gadgets/gadget-sdk/wit/`).
  Cargo virtual workspace, target dir `gadgets/target/`.
- `packages/gadget-sdk/`: TypeScript SDK for gadget frontends.
- `just/`: recipe files imported by `Justfile`. `just --list` shows all.
- `docs/adr/`: architecture decision records.
- `target/` (repo root): gitignored, owned by `stage-bundled-gadgets`.

## Commands

- `just install`: fetch dependencies, generate gitignored assets.
- `just start`: dev mode (`tauri dev`).
- `just fullcycle`: fmt-check, lint, check, test, build. Run before
  committing code changes.
- `just build [--release] [--sign]`: bundle the app. Details under
  Releases.

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
config. Exception: `.devcontainer/` is not MPL-licensed (adapted
third-party files, see `.devcontainer/NOTICE.md`); never add MPL headers
there.

## Gadgets

- Build: plain `cargo build --release` in `gadgets/`; the default target
  `wasm32-wasip2` is set in `gadgets/.cargo/config.toml`. `cargo-component`
  is not used; do not add it or recipes that need it.
- Bindings: `wit_bindgen::generate!` lives in `gadgets/gadget-sdk/`.
  Gadgets use `use torchsnap_gadget_sdk::prelude::*;` and
  `define_gadget!(MyGadget)`.
- WIT tooling: `wasm-tools` via `just check-wit`, `just fmt-wit`.
- Bundling: only ids listed in `gadgets/bundled.toml` ship in the app.
  `just stage-bundled-gadgets` (run by `just build`) empties
  `target/bundled-gadgets/`, rebuilds the listed gadgets and stages their
  `.torchsnap` archives; Tauri bundles that directory. Unlisted gadgets
  under `gadgets/<id>/` still load in debug builds (ADR 0035).
- Storage: gadget code under `<app_data_dir>/gadgets/`, per-gadget state
  under `<app_data_dir>/gadget-home/<gadget-id>/`. SQLite files use
  `.sqlite3` (ADR 0018, ADR 0035).

## Decisions

Record decisions as ADRs in `docs/adr/`:

- Create: `EDITOR=true adrs new "<title>"`, then fill Context, Decision,
  Consequences and set Status to `Accepted`.
- Link changed ADRs both ways: `Amends [N. Title](file)` in the new one,
  `Amended by [N. Title](file)` in the old one, below the status.
- Record only what was decided; no invented rationale.

## Changelog

`CHANGELOG.md` follows Keep a Changelog. Every user-visible change gets
an entry under `[Unreleased]` (`Added`, `Changed`, `Removed`, `Fixed`).

## Releases and signing

- `just build --release --sign`: signs with `APPLE_SIGNING_IDENTITY`
  (default `-`, ad-hoc) and the hardened runtime plus the entitlements in
  `src-tauri/Entitlements.plist` (ADR 0046, 0047). Unsigned local builds
  keep lldb working.
- With a Developer ID and notarization credentials in the environment,
  Tauri notarizes the app and `just notarize-dmg` notarizes the DMG
  (ADR 0049). Variables and checks: README, "Signing macOS builds".
- Releases ship one arm64 DMG uploaded as `Torchsnap.dmg`; the bundle is
  `Torchsnap.app`, identifier `app.torchsnap` (ADR 0048).
