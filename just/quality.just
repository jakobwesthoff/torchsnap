# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file contains all code quality gates: type checking, clippy
# linting, eslint, formatting, and build verification. The `fullcycle`
# recipe runs them all in sequence as a pre-push validation step.

# Run all tests (src-tauri Rust suite + gadget workspace + frontend + tools)
test: test-crates test-gadgets test-frontend test-tools

# Tools under `tools/` that the release depends on carry unittest
# suites in `tools/tests/`. They use only the Python standard library,
# so the CI runner's `python3` is enough.

# Run the tests of the tools under tools/
test-tools:
    PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools/tests -p 'test_*.py'

# Unit tests plus integration tests under `src-tauri/tests/` — the
# latter load the committed `.wasm` fixtures from `tests/fixtures/`,
# so run `just build-test-fixtures` first if you've edited any
# fixture source.

# Run the src-tauri Rust test suite
test-crates:
    cargo test --manifest-path src-tauri/Cargo.toml

# The gadgets/ workspace defaults to wasm32-wasip2 via
# `.cargo/config.toml`, but `cargo test` must produce a host-native
# executable to actually run the tests. Detecting the host triple
# from `rustc -vV` keeps this portable across macOS/Linux/arm64/x64
# without hardcoding.

# Run gadget unit tests (forces host target over the wasm32-wasip2 default)
test-gadgets:
    cd gadgets && cargo test --target "$(rustc -vV | sed -n 's|host: ||p')"

# Tests import application modules, and some of those import the
# generated `src/derived/mascots.json`, so it has to exist first (the
# same reason `check-types` depends on it).

# Run the frontend test suite (Vitest)
test-frontend: asset-mascot-data
    bun run test

# Check compilation (Rust + WIT + WASM gadgets) and types (TypeScript)
check: check-crates check-wit check-gadgets check-types

# Check the Rust crate compiles
check-crates:
    cargo check --manifest-path src-tauri/Cargo.toml

# Type-check the frontend. `src/derived/mascots.json` is a generated
# import, so `asset-mascot-data` has to run before tsc can resolve it.
check-types: asset-mascot-data
    bun run tsc --noEmit

# Lint everything (Rust + WASM gadgets + frontend)
lint: lint-crates lint-gadgets lint-frontend

# Both clippy recipes deny warnings. Every crate in the repo lints
# clean, so anything new is a regression rather than noise added to a
# backlog; deliberately kept items carry a targeted `#[allow(...)]`.

# Lint Rust (clippy, including test and bench targets)
lint-crates:
    cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings

# The gadgets/ workspace defaults to wasm32-wasip2 via
# `.cargo/config.toml`. Clippy runs on the host triple instead — the
# same compilation mode `test-gadgets` uses — so `--all-targets` also
# lints the `#[cfg(test)]` code, which only ever builds host-native.
# Detecting the host triple from `rustc -vV` keeps this portable
# across macOS/Linux/arm64/x64 without hardcoding.

# Lint gadget crates (forces host target over the wasm32-wasip2 default)
lint-gadgets:
    cd gadgets && cargo clippy --workspace --all-targets --target "$(rustc -vV | sed -n 's|host: ||p')" -- -D warnings

# Lint frontend (eslint)
lint-frontend:
    bun run lint

# Format everything (Rust + WASM gadgets + WIT + frontend)
fmt: fmt-crates fmt-gadgets fmt-wit fmt-frontend

# Format Rust (cargo fmt)
fmt-crates:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all

# Format gadget crates (cargo fmt)
fmt-gadgets:
    cargo fmt --manifest-path gadgets/Cargo.toml --all

# Format frontend (prettier)
fmt-frontend:
    bun run fmt

# Check formatting without writing (Rust + WASM gadgets + WIT + frontend)
fmt-check: fmt-check-crates fmt-check-gadgets fmt-check-wit fmt-check-frontend

# Check Rust formatting
fmt-check-crates:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all --check

# Check gadget crate formatting
fmt-check-gadgets:
    cargo fmt --manifest-path gadgets/Cargo.toml --all --check

# Check frontend formatting
fmt-check-frontend:
    bun run fmt:check

# The audit recipes query the advisory databases over the network,
# so they stay out of `fullcycle`. Both check every lockfile before
# failing, so one advisory does not hide the others.

# Check Rust and JS dependencies against published security advisories
audit:
    #!/usr/bin/env bash
    set -uo pipefail
    status=0
    just audit-crates || status=1
    just audit-packages || status=1
    exit "$status"

# The host lockfile and the gadget workspace lockfile cover the code
# that ships. The fixture crates under `src-tauri/tests/fixtures/`
# only produce test inputs and are not audited.

# Check the host and gadget Cargo lockfiles against the RustSec database
audit-crates:
    #!/usr/bin/env bash
    set -uo pipefail
    status=0
    for lockfile in src-tauri/Cargo.lock gadgets/Cargo.lock; do
        echo "== $lockfile"
        cargo audit --file "$lockfile" || status=1
    done
    exit "$status"

# Check the host and every gadget frontend against the npm advisory database
audit-packages:
    #!/usr/bin/env bash
    set -uo pipefail
    status=0
    for project in . gadgets/*/frontend; do
        [ -f "$project/package.json" ] || continue
        echo "== $project"
        bun audit --cwd "$project" || status=1
    done
    exit "$status"

# Run the full quality cycle: format check, lint, type check, test, then build
fullcycle:
    just fmt-check
    just lint
    just check
    just test
    just build-frontend
