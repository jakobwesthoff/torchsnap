# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file handles building WASM gadget crates. Each gadget in
# gadgets/ is a standalone Rust project that compiles to a .wasm
# component targeting wasm32-wasip2.

# The loop keys off `manifest.toml` rather than `Cargo.toml` so the
# shared `gadget-sdk` workspace member (which has a `Cargo.toml` but no
# gadget manifest) is transparently skipped.

WASM_BUILD_DIR := "gadgets/target/wasm32-wasip2/release"
BUNDLED_DIR := "target/bundled-gadgets"

# Build all WASM gadgets
build-gadgets:
    #!/usr/bin/env bash
    set -euo pipefail
    for dir in gadgets/*/; do
        if [ -f "$dir/manifest.toml" ]; then
            name=$(basename "$dir")
            just build-gadget "$name"
        fi
    done

# If the gadget has a frontend (package.json), builds it first with
# Vite. Then compiles the WASM binary and copies the artifact to the
# gadget root so DirectorySource can find it via the manifest's `wasm`
# path during development.

# Build a single WASM gadget by name
build-gadget name:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="gadgets/{{ name }}"

    # Build frontend if present.
    if [ -f "$dir/frontend/package.json" ]; then
        echo "Building frontend for {{ name }}..."
        (cd "$dir/frontend" && bun install && bun run build)
    fi

    # Run cargo from inside the gadget workspace so
    # `gadgets/.cargo/config.toml` (which sets the default
    # target to `wasm32-wasip2`) is picked up and we share
    # the workspace-level `gadgets/target/` build cache
    # across every gadget.
    (cd gadgets && cargo build --manifest-path "{{ name }}/Cargo.toml" --release)

    # Read the wasm filename from the manifest and copy the
    # built artifact to the gadget root directory.
    wasm_file=$(tools/toml-get "$dir/manifest.toml" gadget.wasm)
    # Cargo names the artifact after the crate name with
    # hyphens replaced by underscores.
    crate_name=$(tools/toml-get "$dir/Cargo.toml" package.name | tr '-' '_')
    src="{{ WASM_BUILD_DIR }}/${crate_name}.wasm"
    if [ -f "$src" ]; then
        cp "$src" "$dir/$wasm_file"
        echo "Copied $src → $dir/$wasm_file"
    else
        echo "Warning: expected artifact not found: $src"
        exit 1
    fi

    just package-gadget "{{ name }}"

# Package a gadget directory into a .torchsnap zip archive.
#
# **Blacklist semantics**: every file under the gadget directory
# is included by default. The recipe excludes only known build
# artifacts and source files that shouldn't ship to users. This
# means new manifest features that reference additional files
# (e.g. SQL migrations, asset bundles, fixture data) work
# automatically — gadget authors don't need to update this
# recipe whenever they add a new file kind, and we can't
# silently leave a file out of the archive that the runtime
# loader expects to find.
#
# Excluded paths (build inputs and developer-only artifacts):
#
# - `target/`        — Rust build output, the compiled `.wasm`
#                       lives at the gadget root after
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

# Package a gadget directory into a .torchsnap zip archive
package-gadget name:
    #!/usr/bin/env bash
    set -euo pipefail
    dir="gadgets/{{ name }}"

    # Sanity check: the WASM binary must exist at the path
    # the manifest declares, or the archive would package the
    # frontend without the WASM that drives it.
    wasm_file=$(tools/toml-get "$dir/manifest.toml" gadget.wasm)
    if [ ! -f "$dir/$wasm_file" ]; then
        echo "Error: WASM binary not found: $dir/$wasm_file"
        echo "Run 'just build-gadget {{ name }}' first"
        exit 1
    fi

    archive="gadgets/{{ name }}.torchsnap"

    # Remove any stale archive from a previous run before
    # packing. `zip -r` would otherwise update-in-place, which
    # leaves files from the previous run inside the archive
    # even if they no longer exist on disk.
    rm -f "$archive"

    # Pack everything under the gadget directory using `zip -r`,
    # excluding the blacklist patterns documented above. Paths
    # inside the archive are relative to the gadget root.
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
# `target/bundled-gadgets/` so Tauri's bundler can embed them
# as resources. The list comes from `gadgets/bundled.toml`:
# each entry must match a subdirectory under `gadgets/`.
#
# Behaviour:
#
# - Missing `bundled.toml` → stage zero gadgets and emit a
#   clear warning. A release built in this state still works
#   but ships no system plugins.
# - Duplicate ids in `bundled.toml` → hard error so the
#   mistake surfaces at build time, not at runtime.
# - Listed gadget id that does not exist under `gadgets/` →
#   hard error with the offending id in the message.
#
# The staging directory is wiped before each run so a
# previously-bundled gadget removed from `bundled.toml` is also removed
# from the next bundle.

# Stage whitelisted gadgets into target/bundled-gadgets/ for Tauri bundling
stage-bundled-gadgets:
    #!/usr/bin/env bash
    set -euo pipefail

    stage="{{ BUNDLED_DIR }}"
    mkdir -p "$stage"
    # Wipe stale archives from previous runs so a gadget
    # removed from `bundled.toml` is not silently bundled.
    rm -f "$stage"/*.torchsnap

    # list-bundled-gadgets handles the missing-file and empty-list
    # cases gracefully (warnings to stderr, empty stdout, exit 0).
    # Under `set -e`, a non-zero exit (duplicate id) propagates
    # immediately. The empty-string guard is needed because
    # `mapfile <<< ""` yields a one-element array of empty strings.
    ids_output=$(tools/list-bundled-gadgets gadgets/bundled.toml)
    [ -z "$ids_output" ] && exit 0
    mapfile -t ids <<< "$ids_output"

    # Existence guard: fail early if `bundled.toml` names a
    # gadget that has no source under `gadgets/`.
    for id in "${ids[@]}"; do
        if [ ! -d "gadgets/$id" ]; then
            echo "Error: bundled.toml lists '$id' but gadgets/$id/ does not exist"
            exit 1
        fi
    done

    # Build + copy each listed gadget's archive into the
    # staging dir. `build-gadget` is idempotent and handles
    # frontend + WASM + packaging.
    for id in "${ids[@]}"; do
        just build-gadget "$id"
        cp "gadgets/$id.torchsnap" "$stage/$id.torchsnap"
        echo "Staged → $stage/$id.torchsnap"
    done

# Cargo resolves the workspace from the `gadgets/` directory and
# `gadgets/.cargo/config.toml` supplies the wasm32-wasip2 target by
# default, so no explicit `--target` flag is needed here.

# Check every WASM gadget in the gadgets/ workspace (including gadget-sdk)
check-gadgets:
    cd gadgets && cargo check --workspace

# The `name` argument matches the gadget's *directory* name (e.g.
# `hello-world`), which differs from the crate name
# (`hello-world-gadget`); `--manifest-path` takes a path, so we stay
# directory-based for caller ergonomics.

# Check that a single WASM gadget compiles
check-gadget name:
    cd gadgets && cargo check --manifest-path "{{ name }}/Cargo.toml"

# Each fixture is a standalone wasm32-wasip2 crate under
# `src-tauri/tests/fixtures/`; this recipe compiles them and copies the
# `.wasm` artifact to the gadget root directory (next to the fixture's
# Cargo.toml / README) where the tests look for it.
#
# The `.wasm` artifacts are committed to the repository — this recipe
# is only run by developers when a fixture's source changes or the
# shared WIT world is updated. CI runs the Rust test suite against the
# committed `.wasm`.

# Build the WASM test fixtures used by the src-tauri Rust tests
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
        wasm_file=$(tools/toml-get "$dir/manifest.toml" gadget.wasm)

        if [ -f "$src" ]; then
            cp "$src" "$dir/$wasm_file"
            echo "Copied $src → $dir/$wasm_file"
        else
            echo "Error: expected artifact not found: $src"
            exit 1
        fi
    done

# The WIT world lives under the `torchsnap-gadget-sdk` crate so the
# SDK owns both the `.wit` source and the generated bindings in one
# place.

# Validate that WIT definitions parse correctly
check-wit:
    wasm-tools component wit gadgets/gadget-sdk/wit/ > /dev/null

# Format WIT definitions to canonical form
fmt-wit:
    #!/usr/bin/env bash
    set -euo pipefail
    for f in gadgets/gadget-sdk/wit/*.wit; do
        wasm-tools component wit "$f" -o "$f"
    done

# Check WIT formatting without writing
fmt-check-wit:
    #!/usr/bin/env bash
    set -euo pipefail
    for f in gadgets/gadget-sdk/wit/*.wit; do
        if ! diff -q <(wasm-tools component wit "$f") "$f" > /dev/null 2>&1; then
            echo "WIT not formatted: $f"
            echo "Run 'just fmt-wit' to fix"
            exit 1
        fi
    done
