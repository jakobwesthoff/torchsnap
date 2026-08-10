# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file handles launching the Torchsnap app for local development.
# The main `start` recipe runs the Tauri dev server which combines the
# Vite HMR frontend with the native Rust backend.

# Start Torchsnap in Tauri dev mode (Vite HMR + native window)
start: asset-mascot-data
    bun run tauri dev
