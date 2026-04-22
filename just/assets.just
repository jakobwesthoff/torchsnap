# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file generates all visual and binary assets from their source
# files. It covers mascot image resizing and WebP conversion, app icon
# generation via Tauri's icon generator, and tray icon alpha-mask
# creation. All recipes are incremental — they skip outputs that are
# already up to date relative to their sources.

# Generate all assets (mascots, mascot data, timezone data, app icons, tray icon)
assets: asset-mascots asset-mascot-data asset-timezone-data asset-app-icons asset-tray-icon asset-bang-data

# All Tauri targets (WebKit, WebView2) support WebP natively — no PNG
# output needed.

# Resize mascot source PNGs to WebP at the sizes the frontend needs (incremental)
asset-mascots:
    #!/usr/bin/env bash
    set -euo pipefail
    dst="public/images/mascot"
    mkdir -p "$dst"
    processed=0
    skipped=0
    for file in assets/mascot/*-1024.png; do
        [ -e "$file" ] || continue
        name=$(basename "$file" -1024.png)
        needed=false
        # Currently used sizes: 96 (sidekick 1x), 192 (center 1x /
        # sidekick 2x), 384 (center 2x). Re-enable 128, 256, 512 here
        # if new components need them.
        for size in 384 192 96; do
            webp="$dst/${name}-${size}.webp"
            if [ ! -f "$webp" ] || [ "$file" -nt "$webp" ]; then
                needed=true
                tmp=$(mktemp /tmp/mascot-XXXXXX.png)
                magick "$file" -filter Lanczos -resize ${size}x${size} "$tmp"
                cwebp -q 90 -m 6 -alpha_q 100 -sharp_yuv -metadata none "$tmp" -o "$webp"
                rm -f "$tmp"
            fi
        done
        if [ "$needed" = true ]; then
            echo "Processed $name"
            processed=$((processed + 1))
        else
            skipped=$((skipped + 1))
        fi
    done
    echo "Mascots: ${processed} processed, ${skipped} up to date."

# Trim values are the percentage of transparent space on each edge of
# the 1024×1024 source image — used at runtime to align mascot variants
# so their feet land at the same position regardless of pose.

# Enrich mascots.json with per-variant trim data and write to src/derived/ (incremental)
asset-mascot-data:
    #!/usr/bin/env bash
    set -euo pipefail
    src="assets/mascot/mascots.json"
    dst="src/derived/mascots.json"

    # Regenerate when the source JSON or any source PNG is newer than
    # the output.
    needs_update=false
    if [ ! -f "$dst" ] || [ "$src" -nt "$dst" ]; then
        needs_update=true
    fi
    if [ "$needs_update" = false ]; then
        for png in assets/mascot/*-1024.png; do
            [ -e "$png" ] || continue
            if [ "$png" -nt "$dst" ]; then
                needs_update=true
                break
            fi
        done
    fi
    if [ "$needs_update" = false ]; then
        echo "Mascot data: up to date."
        exit 0
    fi

    echo "Enriching mascots.json with trim data..."

    # Step 1: Build a JSON object mapping variant name → trim percentages.
    # Uses magick identify to find the bounding box of non-transparent
    # pixels in each source PNG.
    trim_json="{"
    first=true
    for png in assets/mascot/*-1024.png; do
        [ -e "$png" ] || continue
        variant=$(basename "$png" -1024.png)
        variant="${variant#snappy-}"

        # Threshold alpha at 75% before computing the bounding box. This
        # discards faint glows, sparkles, and soft shadows that ImageMagick
        # would otherwise include — they're invisible after downscaling to
        # 96/192px but would skew the trim values significantly.
        geom=$(magick identify -format "%@" <(magick "$png" -channel A -threshold 75% +channel PNG:-) 2>/dev/null || echo "1024x1024+0+0")
        IFS='x+' read -r w h x y <<< "$geom"

        top=$(awk "BEGIN { printf \"%.1f\", ($y / 1024) * 100 }")
        right=$(awk "BEGIN { printf \"%.1f\", ((1024 - ($x + $w)) / 1024) * 100 }")
        bottom=$(awk "BEGIN { printf \"%.1f\", ((1024 - ($y + $h)) / 1024) * 100 }")
        left=$(awk "BEGIN { printf \"%.1f\", ($x / 1024) * 100 }")

        [ "$first" = true ] && first=false || trim_json+=","
        trim_json+="\"${variant}\":{\"top\":${top},\"right\":${right},\"bottom\":${bottom},\"left\":${left}}"
    done
    trim_json+="}"

    # Step 2: Merge trim data into the hand-authored JSON. Only entries
    # present in both the source JSON and the trim map receive a trim
    # object — variants without a source PNG are left untouched.
    jq --argjson trims "$trim_json" '
        reduce keys[] as $k (.;
            if $trims[$k] then .[$k].trim = $trims[$k] else . end
        )
    ' "$src" > "$dst"

    echo "Mascot data: done."

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
        # formats (icns, correct sizes, squircle treatment, etc.)
        bunx tauri icon "$source_icon"
    else
        echo "App icons: up to date."
    fi

# Generate the menubar tray icon as a macOS template alpha mask (incremental)
asset-tray-icon:
    #!/usr/bin/env bash
    set -euo pipefail
    src="assets/snappy-tray-template-2-44px.png"
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
