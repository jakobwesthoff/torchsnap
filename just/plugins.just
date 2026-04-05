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
# If the plugin has a frontend (package.json), builds it first
# with Vite. Then compiles the WASM binary and copies the
# artifact to the plugin root so DirectorySource can find it
# via the manifest's `wasm` path during development.
build-plugin name:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="plugins/{{ name }}"

    # Build frontend if present.
    if [ -f "$dir/frontend/package.json" ]; then
        echo "Building frontend for {{ name }}..."
        (cd "$dir/frontend" && bun install && bun run build)
    fi

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
        exit 1
    fi

    just package-plugin "{{ name }}"

# Package a plugin directory into a .torchsnap zip archive.
# Includes manifest.toml and any files referenced by the manifest
# (WASM binary, frontend assets). Build artifacts and source code
# are excluded.
package-plugin name:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="plugins/{{ name }}"

    # Determine which files to include. Always start with the
    # manifest and the WASM binary it references.
    wasm_file=$(grep '^wasm' "$dir/manifest.toml" | sed 's/.*= *"//' | sed 's/".*//')

    if [ ! -f "$dir/$wasm_file" ]; then
        echo "Error: WASM binary not found: $dir/$wasm_file"
        echo "Run 'just build-plugin {{ name }}' first"
        exit 1
    fi

    archive="plugins/{{ name }}.torchsnap"

    # Build the archive from the plugin directory. Paths inside
    # the zip are relative to the plugin root.
    cd "$dir"
    zip -j "../../$archive" "manifest.toml" "$wasm_file"

    # Include frontend assets referenced in the manifest.
    # Each bundle and CSS path is optional — only include files
    # that exist.
    for key in launcher-bundle launcher-css settings-bundle settings-css; do
        path=$(grep "^$key" manifest.toml 2>/dev/null | sed 's/.*= *"//' | sed 's/".*//' || true)
        if [ -n "$path" ] && [ -f "$path" ]; then
            zip "../../$archive" "$path"
            echo "  + $path"
        fi
    done

    echo "Packaged → $archive"

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
