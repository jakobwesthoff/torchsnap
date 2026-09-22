# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file compiles all project artifacts. Torchsnap is a Tauri
# desktop app with a Vite frontend, Rust backend, and WASM gadgets.

# `stage-bundled-gadgets` rebuilds and stages only the whitelisted
# gadgets from `gadgets/bundled.toml` so Tauri's bundler picks them up
# as resources. `build-gadgets` is still run afterwards so every gadget
# under `gadgets/` — bundled or not — has an up-to-date committed
# `.torchsnap` for the dev-directory loader.

# `asset-mascot-data` produces `src/derived/mascots.json`, which the
# frontend imports directly. It is generated rather than tracked, so
# every recipe that compiles the frontend depends on it — that also
# keeps the mascot trim values in step with the source PNGs whenever a
# new mascot is added.

# Tauri code signs the macOS bundle whenever `APPLE_SIGNING_IDENTITY`
# is set, and then enables the hardened runtime with the entitlements
# from `src-tauri/Entitlements.plist` (ADR 0046). `--sign` defaults the
# identity to `-` (ad-hoc) and keeps any identity already set in the
# environment, such as a Developer ID. Without `--sign` the bundle stays
# unsigned unless the environment sets an identity, so lldb can still
# attach to local builds (ADR 0047).

# Build the Tauri app (gadgets + frontend + Rust)
[arg('profile', long='release', value='release')]
[arg('sign', long='sign', value='sign')]
build profile="debug" sign="": asset-mascot-data
    just stage-bundled-gadgets
    just build-gadgets
    {{ if sign == "sign" { "APPLE_SIGNING_IDENTITY=\"${APPLE_SIGNING_IDENTITY:--}\"" } else { "" } }} bun run tauri build {{ if profile == "release" { "" } else { "--debug" } }}

# Build only the frontend (Vite + TypeScript)
build-frontend: asset-mascot-data
    bun run build

# Check the Rust crate compiles
build-check:
    cargo check --manifest-path src-tauri/Cargo.toml
