# This file fetches and installs all external dependencies needed to
# build the project. It covers two dependency trees: the bun-managed
# JS/TS packages and the Rust crates in src-tauri/.

# Install all project dependencies
install: install-packages install-crates

# Install JS dependencies
install-packages:
    bun install

# Fetch Rust dependencies
install-crates:
    cargo fetch --manifest-path src-tauri/Cargo.toml
