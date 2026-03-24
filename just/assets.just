# This file generates all visual and binary assets from their source
# files. It covers mascot image resizing and WebP conversion, app icon
# generation via Tauri's icon generator, and tray icon alpha-mask
# creation. All recipes are incremental — they skip outputs that are
# already up to date relative to their sources.

# Generate all assets (mascots, app icons, tray icon)
assets: asset-mascots asset-app-icons asset-tray-icon

# Resize mascot PNGs to multiple sizes with WebP variants (incremental)
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
        for size in 512 384 256 192 128 96; do
            png="$dst/${name}-${size}.png"
            webp="$dst/${name}-${size}.webp"
            if [ ! -f "$png" ] || [ "$file" -nt "$png" ]; then
                needed=true
                magick "$file" -filter Lanczos -resize ${size}x${size} "$png"
                oxipng -o max --strip safe --alpha "$png"
            fi
            if [ ! -f "$webp" ] || [ "$png" -nt "$webp" ]; then
                needed=true
                cwebp -q 90 -m 6 -alpha_q 100 -sharp_yuv -metadata none "$png" -o "$webp"
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

# Composite mascot onto brand gradient and generate all app icon formats via `tauri icon` (incremental)
asset-app-icons:
    #!/usr/bin/env bash
    set -euo pipefail
    src="assets/mascot/snappy-original-1024.png"
    icons="src-tauri/icons"
    source_icon="$icons/app-icon-source.png"
    if [ -f "$source_icon" ] && [ "$source_icon" -nt "$src" ]; then
        echo "App icons: up to date."
        exit 0
    fi
    echo "Generating app icons..."
    # Create a 1024x1024 gradient canvas with the mascot centered at ~78%
    # size for padding. Force 8-bit depth (Tauri rejects 16-bit PNGs).
    # Uses a warm orange-to-amber gradient matching the torch theme.
    magick -size 1024x1024 gradient:'#f97316'-'#d97706' \
        \( "$src" -filter Lanczos -resize 799x799 \) \
        -gravity center -composite \
        -depth 8 -define png:color-type=6 "$source_icon"
    # Let Tauri's icon generator handle all platform-specific formats
    # (icns, correct sizes, squircle treatment, etc.)
    bunx tauri icon "$source_icon"

# Generate the menubar tray icon as a macOS template alpha mask (incremental)
asset-tray-icon:
    #!/usr/bin/env bash
    set -euo pipefail
    src="assets/mascot/snappy-original-1024.png"
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
    scripts/png-to-alpha-mask "$tmp" "$tray"
    rm -f "$tmp"
