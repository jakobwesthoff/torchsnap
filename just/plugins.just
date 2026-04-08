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
#
# **Blacklist semantics**: every file under the plugin directory
# is included by default. The recipe excludes only known build
# artifacts and source files that shouldn't ship to users. This
# means new manifest features that reference additional files
# (e.g. SQL migrations, asset bundles, fixture data) work
# automatically — plugin authors don't need to update this
# recipe whenever they add a new file kind, and we can't
# silently leave a file out of the archive that the runtime
# loader expects to find.
#
# Excluded paths (build inputs and developer-only artifacts):
#
# - `target/`        — Rust build output, the compiled `.wasm`
#                       lives at the plugin root after
#                       `build-plugin` copies it there
# - `Cargo.toml`,
#   `Cargo.lock`     — Rust build manifests, not runtime config
# - `src/`           — Rust source, only the compiled binary
#                       ships
# - `frontend/node_modules/` — JS dependencies
# - `frontend/src/`  — TypeScript source, only the bundled
#                       `frontend/dist/` output ships
# - `frontend/styles/` — Tailwind input compiled into
#                         `frontend/dist/*.css`
# - `frontend/{package.json,tsconfig.json,vite.config.ts,env.d.ts,bun.lock}`
#                    — frontend build config
# - `.gitignore`,
#   `.git/`          — VCS metadata
# - `*.torchsnap`    — never include a stale archive in a new
#                       archive
package-plugin name:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="plugins/{{ name }}"

    # Sanity check: the WASM binary must exist at the path
    # the manifest declares, or the archive would package the
    # frontend without the WASM that drives it.
    wasm_file=$(grep '^wasm' "$dir/manifest.toml" | sed 's/.*= *"//' | sed 's/".*//')
    if [ ! -f "$dir/$wasm_file" ]; then
        echo "Error: WASM binary not found: $dir/$wasm_file"
        echo "Run 'just build-plugin {{ name }}' first"
        exit 1
    fi

    archive="plugins/{{ name }}.torchsnap"

    # Remove any stale archive from a previous run before
    # packing. `zip -r` would otherwise update-in-place, which
    # leaves files from the previous run inside the archive
    # even if they no longer exist on disk.
    rm -f "$archive"

    # Pack everything under the plugin directory using `zip -r`,
    # excluding the blacklist patterns documented above. Paths
    # inside the archive are relative to the plugin root.
    cd "$dir"
    zip -r "../../$archive" . \
        -x 'target/*' \
        -x 'Cargo.toml' \
        -x 'Cargo.lock' \
        -x 'src/*' \
        -x 'frontend/node_modules/*' \
        -x 'frontend/src/*' \
        -x 'frontend/styles/*' \
        -x 'frontend/package.json' \
        -x 'frontend/tsconfig.json' \
        -x 'frontend/vite.config.ts' \
        -x 'frontend/env.d.ts' \
        -x 'frontend/bun.lock' \
        -x '.gitignore' \
        -x '.git/*' \
        -x '*.torchsnap'

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
