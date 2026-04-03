# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file contains all code quality gates: type checking, clippy
# linting, eslint, formatting, and build verification. The `fullcycle`
# recipe runs them all in sequence as a pre-push validation step.

# Check compilation (Rust + WASM plugins) and types (TypeScript)
check: check-crates check-plugins check-types

# Check the Rust crate compiles
check-crates:
    cargo check --manifest-path src-tauri/Cargo.toml

# Type-check the frontend
check-types:
    bun run tsc --noEmit

# Lint everything (Rust + frontend)
lint: lint-crates lint-frontend

# Lint Rust (clippy)
lint-crates:
    cargo clippy --manifest-path src-tauri/Cargo.toml

# Lint frontend (eslint)
lint-frontend:
    bun run lint

# Format everything (Rust + frontend)
fmt: fmt-crates fmt-frontend

# Format Rust (cargo fmt)
fmt-crates:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all

# Format frontend (prettier)
fmt-frontend:
    bun run fmt

# Check formatting without writing (Rust + frontend)
fmt-check: fmt-check-crates fmt-check-frontend

# Check Rust formatting
fmt-check-crates:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all --check

# Check frontend formatting
fmt-check-frontend:
    bun run fmt:check

# Run the full quality cycle: format check, lint, type check, then build
fullcycle:
    just fmt-check
    just lint
    just check
    just build-frontend
