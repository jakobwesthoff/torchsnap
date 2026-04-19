# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file fetches and installs all external dependencies needed to
# build the project. It covers two dependency trees: the bun-managed
# JS/TS packages and the Rust crates in src-tauri/.

# Install all project dependencies
install: assets install-packages install-crates

# Install JS dependencies
install-packages:
    bun install

# Fetch Rust dependencies for both workspaces: the native
# host under `src-tauri/` and the plugin workspace under
# `plugins/`. They resolve independently because the two
# trees target different triples.
install-crates:
    cargo fetch --manifest-path src-tauri/Cargo.toml
    cargo fetch --manifest-path plugins/Cargo.toml
