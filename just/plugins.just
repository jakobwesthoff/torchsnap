# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file handles building WASM plugin crates. Each plugin in
# plugins/ is a standalone cargo-component project that compiles
# to a .wasm component targeting wasm32-wasip2.

# Build all WASM plugins
build-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in plugins/*/; do
        if [ -f "$dir/Cargo.toml" ]; then
            name=$(basename "$dir")
            echo "Building plugin: $name"
            cargo component build --manifest-path "$dir/Cargo.toml" --release
        fi
    done

# Build a single WASM plugin by name
build-plugin name:
    cargo component build --manifest-path "plugins/{{ name }}/Cargo.toml" --release

# Check all WASM plugins compile
check-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in plugins/*/; do
        if [ -f "$dir/Cargo.toml" ]; then
            name=$(basename "$dir")
            echo "Checking plugin: $name"
            cargo component check --manifest-path "$dir/Cargo.toml"
        fi
    done

# Check a single WASM plugin compiles
check-plugin name:
    cargo component check --manifest-path "plugins/{{ name }}/Cargo.toml"
