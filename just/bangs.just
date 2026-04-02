# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Download the DuckDuckGo bang database and store it as a derived asset
# for the Rust build. The file is baked into the binary via include_str!
# as a fallback when the plugin cannot fetch a fresh copy at runtime.

# Download DuckDuckGo bang database into src-tauri/derived/
asset-bang-data:
    #!/usr/bin/env bash
    set -euo pipefail
    dst="src-tauri/derived/bang.json"
    mkdir -p "$(dirname "$dst")"
    echo "Downloading DuckDuckGo bang database..."
    curl -sL 'https://duckduckgo.com/bang.js' -o "$dst"
    echo "Bang data: done ($(wc -l < "$dst" | tr -d ' ') lines)."
