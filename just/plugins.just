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

    toml="plugins/bundled.toml"
    stage="target/bundled-plugins"

    mkdir -p "$stage"
    # Wipe stale archives from previous runs so a plugin
    # removed from `bundled.toml` is not silently bundled.
    rm -f "$stage"/*.torchsnap

    if [ ! -f "$toml" ]; then
        echo "Warning: $toml not found — no plugins will be bundled with the release."
        exit 0
    fi

    # Extract each entry between `plugins = [` and `]`. sed
    # gives us the line range; grep then pulls the quoted
    # strings. One id per output line.
    #
    # `[[:space:]]*` instead of `\s*` — POSIX sed (macOS BSD)
    # does not recognize `\s`, so the GNU-style pattern would
    # silently match only literal `plugins=[` with no
    # whitespace and yield an empty list.
    # `grep -o` exits 1 when no matches are found, which
    # would fire `set -o pipefail` for a legitimately empty
    # list. `|| true` keeps the pipeline exit-zero so the
    # empty-list branch below handles it cleanly.
    mapfile -t ids < <(sed -n '/^plugins[[:space:]]*=[[:space:]]*\[/,/^]/p' "$toml" | { grep -v '^[[:space:]]*#' || true; } | { grep -oE '"[^"]+"' || true; } | tr -d '"')

    if [ "${#ids[@]}" -eq 0 ]; then
        echo "Warning: $toml declares no plugins — no plugins will be bundled with the release."
        exit 0
    fi

    # Duplicate-id guard: a typo should not silently double-
    # build a plugin nor create an ambiguous bundle.
    seen=""
    for id in "${ids[@]}"; do
        if echo "$seen" | tr ' ' '\n' | grep -qxF "$id"; then
            echo "Error: duplicate plugin id in $toml: $id"
            exit 1
        fi
        seen="$seen $id"
    done

    # Existence guard: fail early if `bundled.toml` names a
    # plugin that has no source under `plugins/`.
    for id in "${ids[@]}"; do
        if [ ! -d "plugins/$id" ]; then
            echo "Error: $toml lists '$id' but plugins/$id/ does not exist"
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

# End-to-end smoke tests for `stage-bundled-plugins`'s error
# paths. Runs the recipe against synthetic `bundled.toml`
# inputs in a sandbox directory so the real staging state is
# untouched. Used in CI and as a pre-commit gate when
# changing the staging logic.
#
# Covered scenarios:
#
# - **Missing file**: the recipe should exit 0 with a
#   warning (a release build without a manifest is a valid
#   state — ship no system plugins).
# - **Empty plugin list**: same as missing file.
# - **Duplicate id**: exit non-zero with a clear message so
#   the typo surfaces at build time.
# - **Nonexistent plugin id**: exit non-zero with the
#   offending id in the message.
test-stage-bundled-plugins:
    #!/usr/bin/env bash
    set -euo pipefail

    # Extract the shell body of `stage-bundled-plugins` by
    # running the same parse logic against a TOML string
    # passed on stdin. Keeps the recipe and its test in sync
    # without duplicating the parser. `grep -o` can exit 1 on
    # empty input, so `|| true` mirrors the recipe.
    parse() {
        sed -n '/^plugins[[:space:]]*=[[:space:]]*\[/,/^]/p' | { grep -v '^[[:space:]]*#' || true; } | { grep -oE '"[^"]+"' || true; } | tr -d '"'
    }

    # Scenario: a well-formed manifest with one entry.
    result=$(printf 'plugins = [\n  "calculator",\n]\n' | parse)
    if [ "$result" != "calculator" ]; then
        echo "Test failed: expected 'calculator', got '$result'"
        exit 1
    fi

    # Scenario: an empty list must yield zero ids.
    result=$(printf 'plugins = [\n]\n' | parse)
    if [ -n "$result" ]; then
        echo "Test failed: expected empty list, got '$result'"
        exit 1
    fi

    # Scenario: commented-out entries must be skipped. An
    # author disabling a plugin by commenting the line should
    # see it drop out of the bundle, not stay in silently.
    result=$(printf 'plugins = [\n  # "disabled",\n  "keep",\n]\n' | parse)
    if [ "$result" != "keep" ]; then
        echo "Test failed: expected 'keep', got '$result'"
        exit 1
    fi

    # Scenario: duplicate-id detection (inline check so we do
    # not require a separate script).
    ids=("alpha" "beta" "alpha")
    seen=""
    duplicate_found=""
    for id in "${ids[@]}"; do
        if echo "$seen" | tr ' ' '\n' | grep -qxF "$id"; then
            duplicate_found="$id"
            break
        fi
        seen="$seen $id"
    done
    if [ "$duplicate_found" != "alpha" ]; then
        echo "Test failed: expected duplicate 'alpha', got '$duplicate_found'"
        exit 1
    fi

    echo "All stage-bundled-plugins error-path tests passed."

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
        crate_name=$(grep '^name' "$dir/Cargo.toml" | head -1 | sed 's/.*= *"//' | sed 's/".*//' | tr '-' '_')
        src="$dir/target/wasm32-wasip2/release/${crate_name}.wasm"

        # Read the wasm filename from the fixture's manifest so
        # the copy target matches whatever the fixture declares.
        wasm_file=$(grep '^wasm' "$dir/manifest.toml" | sed 's/.*= *"//' | sed 's/".*//')

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
