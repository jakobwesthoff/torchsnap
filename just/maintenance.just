# This file removes generated artifacts and temporary files.

# Remove all build artifacts
clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    rm -rf dist
    rm -rf node_modules
