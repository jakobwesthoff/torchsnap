# This file compiles all project artifacts. Torchsnap is a single
# Tauri desktop app with a Vite frontend and Rust backend.

# Build the Tauri app (frontend + Rust)
[arg('profile', long='release', value='release')]
build profile="debug":
    bun run tauri build {{ if profile == "release" { "" } else { "--debug" } }}

# Build only the frontend (Vite + TypeScript)
build-frontend:
    bun run build

# Check the Rust crate compiles
build-check:
    cargo check --manifest-path src-tauri/Cargo.toml
