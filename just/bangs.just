# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Download the DuckDuckGo bang database into the bangs
# gadget's bundled assets directory. The WASM guest reads the
# file via the `assets::read` host import as a fallback when
# the gadget cannot fetch a fresh copy from DDG at runtime.
# The file ships inside the `.torchsnap` archive produced by
# `just build-gadget bangs`.

# Download DuckDuckGo bang database into gadgets/bangs/assets/
asset-bang-data:
    #!/usr/bin/env bash
    set -euo pipefail
    dst="gadgets/bangs/assets/bang.json"
    mkdir -p "$(dirname "$dst")"
    echo "Downloading DuckDuckGo bang database..."
    curl -sL 'https://duckduckgo.com/bang.js' -o "$dst"
    echo "Bang data: done ($(wc -l < "$dst" | tr -d ' ') lines)."
