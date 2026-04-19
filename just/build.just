# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file compiles all project artifacts. Torchsnap is a Tauri
# desktop app with a Vite frontend, Rust backend, and WASM plugins.

# `stage-bundled-plugins` rebuilds and stages only the whitelisted
# plugins from `plugins/bundled.toml` so Tauri's bundler picks them up
# as resources. `build-plugins` is still run afterwards so every plugin
# under `plugins/` — bundled or not — has an up-to-date committed
# `.torchsnap` for the dev-directory loader.

# Build the Tauri app (plugins + frontend + Rust)
[arg('profile', long='release', value='release')]
build profile="debug":
    just stage-bundled-plugins
    just build-plugins
    bun run tauri build {{ if profile == "release" { "" } else { "--debug" } }}

# Build only the frontend (Vite + TypeScript)
build-frontend:
    bun run build

# Check the Rust crate compiles
build-check:
    cargo check --manifest-path src-tauri/Cargo.toml
