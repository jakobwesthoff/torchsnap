# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file removes generated artifacts and temporary files.

# Remove all build artifacts
clean:
    cargo clean --manifest-path src-tauri/Cargo.toml
    cargo clean --manifest-path gadgets/gadget-sdk/Cargo.toml
    rm -rf gadgets/target
    rm -rf target/bundled-gadgets
    rm -rf dist
    rm -rf node_modules
    rm -f *.tsbuildinfo
