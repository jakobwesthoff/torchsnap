# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file handles building WASM plugin crates. Each plugin in
# plugins/ is a standalone Rust project that compiles to a .wasm
# component targeting wasm32-wasip2.

# Build all WASM plugins
#
# The loop keys off `manifest.toml` rather than `Cargo.toml`
# so the shared `plugin-sdk` workspace member (no manifest)
# is transparently skipped once PR 2 lands. Today all four
# plugin directories carry both files, so the behaviour
# is identical.
build-plugins:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in plugins/*/; do
        if [ -f "$dir/manifest.toml" ]; then
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

    # Run cargo from inside the plugin workspace so
    # `plugins/.cargo/config.toml` (which sets the default
    # target to `wasm32-wasip2`) is picked up and we share
    # the workspace-level `plugins/target/` build cache
    # across every plugin.
    (cd plugins && cargo build --manifest-path "{{ name }}/Cargo.toml" --release)

    # Read the wasm filename from the manifest and copy the
    # built artifact to the plugin root directory.
    wasm_file=$(tools/toml-get "$dir/manifest.toml" plugin.wasm)
    # Cargo names the artifact after the crate name with
    # hyphens replaced by underscores.
    crate_name=$(tools/toml-get "$dir/Cargo.toml" package.name | tr '-' '_')
    src="plugins/target/wasm32-wasip2/release/${crate_name}.wasm"
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
    wasm_file=$(tools/toml-get "$dir/manifest.toml" plugin.wasm)
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

# Stage the curated set of bundled WASM plugins into
# `target/bundled-plugins/` so Tauri's bundler can embed them
# as resources. The list comes from `plugins/bundled.toml`:
# each entry must match a subdirectory under `plugins/`.
#
# Behaviour:
#
# - Missing `bundled.toml` → stage zero plugins and emit a
#   clear warning. A release built in this state still works
#   but ships no system plugins.
# - Duplicate ids in `bundled.toml` → hard error so the
#   mistake surfaces at build time, not at runtime.
# - Listed plugin id that does not exist under `plugins/` →
#   hard error with the offending id in the message.
#
# The staging directory is wiped before each run so a
# previously-bundled plugin removed from `bundled.toml` is
# also removed from the next bundle.
stage-bundled-plugins:
    #!/usr/bin/env bash
    set -euo pipefail

    stage="target/bundled-plugins"
    mkdir -p "$stage"
    # Wipe stale archives from previous runs so a plugin
    # removed from `bundled.toml` is not silently bundled.
    rm -f "$stage"/*.torchsnap

    # list-bundled-plugins handles the missing-file and empty-list
    # cases gracefully (warnings to stderr, empty stdout, exit 0).
    # Under `set -e`, a non-zero exit (duplicate id) propagates
    # immediately. The empty-string guard is needed because
    # `mapfile <<< ""` yields a one-element array of empty strings.
    ids_output=$(tools/list-bundled-plugins plugins/bundled.toml)
    [ -z "$ids_output" ] && exit 0
    mapfile -t ids <<< "$ids_output"

    # Existence guard: fail early if `bundled.toml` names a
    # plugin that has no source under `plugins/`.
    for id in "${ids[@]}"; do
        if [ ! -d "plugins/$id" ]; then
            echo "Error: bundled.toml lists '$id' but plugins/$id/ does not exist"
            exit 1
        fi
    done

    # Build + copy each listed plugin's archive into the
    # staging dir. `build-plugin` is idempotent and handles
    # frontend + WASM + packaging.
    for id in "${ids[@]}"; do
        just build-plugin "$id"
        cp "plugins/$id.torchsnap" "$stage/$id.torchsnap"
        echo "Staged → $stage/$id.torchsnap"
    done

# Check every workspace member (including plugin-sdk once
# PR 2 lands). Cargo resolves the workspace from the
# `plugins/` directory and `plugins/.cargo/config.toml`
# supplies the wasm32-wasip2 target by default, so no
# explicit `--target` flag is needed here.
check-plugins:
    cd plugins && cargo check --workspace

# Check a single WASM plugin compiles. The `name` argument
# matches the plugin's *directory* name (e.g. `hello-world`),
# which differs from the crate name (`hello-world-plugin`);
# `--manifest-path` takes a path, so we stay directory-based
# for caller ergonomics.
check-plugin name:
    cd plugins && cargo check --manifest-path "{{ name }}/Cargo.toml"

# Build the WASM test fixtures used by src-tauri's Rust test
# suite. Each fixture is a standalone wasm32-wasip2 crate under
# `src-tauri/tests/fixtures/`; this recipe compiles them and
# copies the `.wasm` artifact to the plugin root directory (next
# to the fixture's Cargo.toml / README) where the tests look for
# it.
#
# The `.wasm` artifacts are committed to the repository — this
# recipe is only run by developers when a fixture's source
# changes or the shared WIT world is updated. CI runs the Rust
# test suite against the committed `.wasm`.
build-test-fixtures:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in src-tauri/tests/fixtures/*/; do
        if [ ! -f "$dir/Cargo.toml" ]; then
            continue
        fi
        name=$(basename "$dir")
        echo "Building test fixture: $name"
        cargo build --manifest-path "$dir/Cargo.toml" --target wasm32-wasip2 --release

        # Resolve the target directory. These fixture crates sit
        # outside any workspace, so cargo writes their build
        # output to `$dir/target/` — next to the crate's own
        # Cargo.toml. The artifact filename matches the crate
        # name with hyphens replaced by underscores.
        crate_name=$(tools/toml-get "$dir/Cargo.toml" package.name | tr '-' '_')
        src="$dir/target/wasm32-wasip2/release/${crate_name}.wasm"

        # Read the wasm filename from the fixture's manifest so
        # the copy target matches whatever the fixture declares.
        wasm_file=$(tools/toml-get "$dir/manifest.toml" plugin.wasm)

        if [ -f "$src" ]; then
            cp "$src" "$dir/$wasm_file"
            echo "Copied $src → $dir/$wasm_file"
        else
            echo "Error: expected artifact not found: $src"
            exit 1
        fi
    done

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
