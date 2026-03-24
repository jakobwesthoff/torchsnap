# This file handles launching the Torchsnap app for local development.
# The main `start` recipe runs the Tauri dev server which combines the
# Vite HMR frontend with the native Rust backend.

# Start Torchsnap in Tauri dev mode (Vite HMR + native window)
start:
    bun run tauri dev
