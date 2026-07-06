# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file contains all code quality gates: type checking, clippy
# linting, eslint, formatting, and build verification. The `fullcycle`
# recipe runs them all in sequence as a pre-push validation step.

# Run all tests (src-tauri Rust suite + gadget workspace)
test: test-crates test-gadgets

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

# Check compilation (Rust + WIT + WASM gadgets) and types (TypeScript)
check: check-crates check-wit check-gadgets check-types

# Check the Rust crate compiles
check-crates:
    cargo check --manifest-path src-tauri/Cargo.toml

# Type-check the frontend
check-types:
    bun run tsc --noEmit

# Lint everything (Rust + WASM gadgets + frontend)
lint: lint-crates lint-gadgets lint-frontend

# Lint Rust (clippy)
lint-crates:
    cargo clippy --manifest-path src-tauri/Cargo.toml

# The gadgets/ workspace defaults to wasm32-wasip2 via
# `.cargo/config.toml`. Clippy runs on the host triple instead — the
# same compilation mode `test-gadgets` uses — so `--all-targets` also
# lints the `#[cfg(test)]` code, which only ever builds host-native.
# Detecting the host triple from `rustc -vV` keeps this portable
# across macOS/Linux/arm64/x64 without hardcoding.

# Lint gadget crates (forces host target over the wasm32-wasip2 default)
lint-gadgets:
    cd gadgets && cargo clippy --workspace --all-targets --target "$(rustc -vV | sed -n 's|host: ||p')"

# Lint frontend (eslint)
lint-frontend:
    bun run lint

# Format everything (Rust + WIT + frontend)
fmt: fmt-crates fmt-wit fmt-frontend

# Format Rust (cargo fmt)
fmt-crates:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all

# Format frontend (prettier)
fmt-frontend:
    bun run fmt

# Check formatting without writing (Rust + WIT + frontend)
fmt-check: fmt-check-crates fmt-check-wit fmt-check-frontend

# Check Rust formatting
fmt-check-crates:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all --check

# Check frontend formatting
fmt-check-frontend:
    bun run fmt:check

# Run the full quality cycle: format check, lint, type check, test, then build
fullcycle:
    just fmt-check
    just lint
    just check
    just test
    just build-frontend
