# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file contains all code quality gates: type checking, clippy
# linting, and formatting. The `fullcycle` recipe runs them all in
# sequence as a pre-push validation step.

# Check compilation (Rust) and types (TypeScript)
check: check-crates check-types

# Check the Rust crate compiles
check-crates:
    cargo check --manifest-path src-tauri/Cargo.toml

# Type-check the frontend
check-types:
    bun run tsc --noEmit

# Lint Rust (clippy) — runs all checks, fails if any fail
lint:
    cargo clippy --manifest-path src-tauri/Cargo.toml

# Format Rust (cargo fmt) — runs all checks, fails if any fail
fmt:
    cargo fmt --manifest-path src-tauri/Cargo.toml --all

# Run the full quality cycle: format, lint, check, then build frontend
fullcycle:
    just fmt
    just lint
    just check
    just build-frontend
