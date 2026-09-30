# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file generates all visual and binary assets from their source
# files. It covers app icon generation via Tauri's icon generator, the
# document icon, tray icon alpha-mask creation, and the timezone data.
# All recipes are incremental — they skip outputs that are already up
# to date relative to their sources.
#
# The mascot images in public/images/mascot/ and their data in
# src/derived/mascots.json come from the torchsnap-mascot repository
# (`just export <torchsnap checkout>` there) and are tracked as they are.

# Generate all assets (timezone data, app icons, tray icon, bang data)
assets: asset-timezone-data asset-app-icons asset-tray-icon asset-bang-data

# Composite mascot onto brand gradient and generate all app icon formats via `tauri icon` (incremental)
asset-app-icons:
    #!/usr/bin/env bash
    set -euo pipefail
    src="assets/mascot/snappy-original-1024.png"
    icons="src-tauri/icons"
    source_icon="$icons/app-icon-source.png"

    # The recipe runs in two stages: first compose the 1024x1024
    # source icon from the mascot and the brand gradient, then invoke
    # `tauri icon` to fan it out into every platform-specific format.
    # Each stage is checked independently so a partial failure in
    # stage 2 re-runs only stage 2 on the next invocation, rather than
    # locking in a half-populated icons/ directory.

    # Stage 1: (re)build the composite source icon when missing or
    # older than the mascot source.
    if [ ! -f "$source_icon" ] || [ "$src" -nt "$source_icon" ]; then
        echo "Composing app icon source..."
        # The mascot sits at ~78% of the 1024px canvas for padding on
        # top of a warm orange-to-amber torch-theme gradient. 8-bit
        # depth is forced because Tauri rejects 16-bit PNGs.
        magick -size 1024x1024 gradient:'#f97316'-'#d97706' \
            \( "$src" -filter Lanczos -resize 799x799 \) \
            -gravity center -composite \
            -depth 8 -define png:color-type=6 "$source_icon"
    fi

    # Stage 2: regenerate platform-specific icons when any output
    # declared in tauri.conf.json is missing or older than the source.
    # That file is the authoritative contract with the bundler — keep
    # the list below in sync with its `bundle.icon` entry.
    required_icons=(
        "$icons/32x32.png"
        "$icons/128x128.png"
        "$icons/128x128@2x.png"
        "$icons/icon.icns"
        "$icons/icon.ico"
    )
    needs_regen=false
    for icon in "${required_icons[@]}"; do
        if [ ! -f "$icon" ] || [ "$source_icon" -nt "$icon" ]; then
            needs_regen=true
            break
        fi
    done

    if [ "$needs_regen" = true ]; then
        echo "Generating app icons..."
        # Let Tauri's icon generator handle all platform-specific
        # formats (icns, correct sizes, squircle treatment, etc.).
        # `bun run` uses the project's `@tauri-apps/cli` through the
        # `tauri` script in package.json. `bunx tauri` would fetch Tauri's
        # legacy pre-1.0 `tauri` package from npm when node_modules
        # is missing.
        bun run tauri icon "$source_icon"
    else
        echo "App icons: up to date."
    fi

# The document icon Finder shows for `.torchsnap` files: a white page
# with a folded corner carrying the app icon. `iconutil` only exists on
# macOS, which is also the only platform that uses the `.icns`.

# Generate the `.torchsnap` document icon from the app icon source (macOS, incremental)
[macos]
asset-document-icon: asset-app-icons
    #!/usr/bin/env bash
    set -euo pipefail
    source_icon="src-tauri/icons/app-icon-source.png"
    icns="src-tauri/icons/gadget-document.icns"
    if [ -f "$icns" ] && [ "$icns" -nt "$source_icon" ]; then
        echo "Document icon: up to date."
        exit 0
    fi

    echo "Generating document icon..."
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT

    # 1024x1024 page: rounded white sheet with a light border and a
    # folded top-right corner, app icon centered in its lower part.
    magick -size 1024x1024 xc:none \
        -fill white -stroke '#d4d4d8' -strokewidth 8 \
        -draw "path 'M 208,72 L 648,72 L 840,264 L 840,920 Q 840,952 808,952 L 240,952 Q 208,952 208,920 L 208,104 Q 208,72 240,72 Z'" \
        -fill '#e4e4e7' -draw "path 'M 648,72 L 648,232 Q 648,264 680,264 L 840,264 Z'" \
        \( "$source_icon" -filter Lanczos -resize 440x440 \) \
        -gravity center -geometry +0+96 -composite \
        -depth 8 -define png:color-type=6 "$work/page.png"

    iconset="$work/gadget-document.iconset"
    mkdir "$iconset"
    for size in 16 32 128 256 512; do
        magick "$work/page.png" -filter Lanczos -resize "${size}x${size}" "$iconset/icon_${size}x${size}.png"
        double=$((size * 2))
        magick "$work/page.png" -filter Lanczos -resize "${double}x${double}" "$iconset/icon_${size}x${size}@2x.png"
    done
    iconutil -c icns "$iconset" -o "$icns"
    echo "Document icon: done."

# Generate the menubar tray icon as a macOS template alpha mask (incremental)
asset-tray-icon:
    #!/usr/bin/env bash
    set -euo pipefail
    src="assets/snappy-tray-template-3-44px.png"
    tray="src-tauri/icons/tray-icon-template.png"
    if [ -f "$tray" ] && [ "$tray" -nt "$src" ]; then
        echo "Tray icon: up to date."
        exit 0
    fi
    echo "Generating tray icon..."
    # First resize to a small template size (44x44 for @2x), then
    # convert to an alpha mask where luminance becomes opacity.
    tmp=$(mktemp /tmp/tray-resized-XXXXXX.png)
    magick "$src" -filter Lanczos -resize 44x44 "$tmp"
    tools/png-to-alpha-mask "$tmp" "$tray"
    rm -f "$tmp"

# The app icon candidates in `assets/app-icon/` (see its README) are
# tracked and not part of `assets`: rebuilding them needs uv, librsvg
# (`rsvg-convert`) and Xcode's Icon Composer.

# Rebuild the app icon candidates in assets/app-icon/ from the Snappy emblem
app-icon-candidates:
    tools/flatten-emblem assets/snappy-emblem-feathered.svg assets/app-icon/snappy-emblem-flat.svg
    tools/build-app-icons

# Used by the nighttime detection code to compute sunset/sunrise times
# for the user's timezone.

# Parse IANA zone.tab into a timezone → [lat, lon] JSON lookup table (incremental)
asset-timezone-data:
    #!/usr/bin/env bash
    set -euo pipefail
    src="/usr/share/zoneinfo/zone.tab"
    dst="src/derived/timezone-coordinates.json"

    if [ ! -f "$src" ]; then
        echo "Error: $src not found. This recipe requires the system timezone database."
        exit 1
    fi

    if [ -f "$dst" ] && [ "$dst" -nt "$src" ]; then
        echo "Timezone data: up to date."
        exit 0
    fi

    echo "Generating timezone coordinate data..."
    tools/generate-timezone-coords "$src" > "$dst"
    echo "Timezone data: done."
