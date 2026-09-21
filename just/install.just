# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file fetches and installs all external dependencies needed to
# build the project. It covers the bun-managed JS/TS packages of the
# host and of every gadget frontend, and the Rust crates in src-tauri/
# and gadgets/.

# Install all project dependencies
install: assets install-packages install-crates

# Each gadget frontend is its own bun project with its own lockfile.
# The gadget crates' Cargo builds read from these trees as well
# (`gadgets/emoji-picker/build.rs` embeds `emojibase-data` from
# `frontend/node_modules/`), so `check-gadgets`, `lint-gadgets` and
# `test-gadgets` need them installed, not only `build-gadget`.

# Install JS dependencies for the host and every gadget frontend
install-packages:
    #!/usr/bin/env bash
    set -euo pipefail
    bun install
    for frontend in gadgets/*/frontend; do
        [ -f "$frontend/package.json" ] || continue
        bun install --cwd "$frontend"
    done

# The native host under `src-tauri/` and the gadget workspace under
# `gadgets/` resolve independently because the two trees target
# different triples.

# Fetch Rust dependencies for both the src-tauri/ and gadgets/ workspaces
install-crates:
    cargo fetch --manifest-path src-tauri/Cargo.toml
    cargo fetch --manifest-path gadgets/Cargo.toml
