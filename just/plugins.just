# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file handles building WASM plugin crates. Each plugin in
# plugins/ is a standalone Rust project that compiles to a .wasm
# component targeting wasm32-wasip2.

# Build all WASM plugins
build-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in plugins/*/; do
        if [ -f "$dir/Cargo.toml" ]; then
            name=$(basename "$dir")
            just build-plugin "$name"
        fi
    done

# Build a single WASM plugin by name.
# After compilation, copies the .wasm artifact to the plugin
# root so DirectorySource can find it via the manifest's `wasm`
# path during development.
build-plugin name:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="plugins/{{ name }}"
    cargo build --manifest-path "$dir/Cargo.toml" --target wasm32-wasip2 --release

    # Read the wasm filename from the manifest and copy the
    # built artifact to the plugin root directory.
    wasm_file=$(grep '^wasm' "$dir/manifest.toml" | sed 's/.*= *"//' | sed 's/".*//')
    # Cargo names the artifact after the crate name with
    # hyphens replaced by underscores.
    crate_name=$(grep '^name' "$dir/Cargo.toml" | head -1 | sed 's/.*= *"//' | sed 's/".*//' | tr '-' '_')
    src="$dir/target/wasm32-wasip2/release/${crate_name}.wasm"
    if [ -f "$src" ]; then
        cp "$src" "$dir/$wasm_file"
        echo "Copied $src → $dir/$wasm_file"
    else
        echo "Warning: expected artifact not found: $src"
    fi

# Check all WASM plugins compile
check-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in plugins/*/; do
        if [ -f "$dir/Cargo.toml" ]; then
            name=$(basename "$dir")
            echo "Checking plugin: $name"
            cargo check --manifest-path "$dir/Cargo.toml" --target wasm32-wasip2
        fi
    done

# Check a single WASM plugin compiles
check-plugin name:
    cargo check --manifest-path "plugins/{{ name }}/Cargo.toml" --target wasm32-wasip2

# Validate WIT definitions parse correctly
check-wit:
    wasm-tools component wit wit/ > /dev/null

# Format WIT definitions to canonical form
fmt-wit:
    #!/usr/bin/env bash
    set -euo pipefail
    for f in wit/*.wit; do
        wasm-tools component wit "$f" -o "$f"
    done

# Check WIT formatting without writing
fmt-check-wit:
    #!/usr/bin/env bash
    set -euo pipefail
    for f in wit/*.wit; do
        if ! diff -q <(wasm-tools component wit "$f") "$f" > /dev/null 2>&1; then
            echo "WIT not formatted: $f"
            echo "Run 'just fmt-wit' to fix"
            exit 1
        fi
    done
