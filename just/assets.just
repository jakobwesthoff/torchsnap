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

# The app icon is the candidate `app_icon` of `assets/app-icon/` (see
# its README). Xcode's tools compile it into tracked files in
# `assets/app-icon/compiled/` (`app-icon-compile`); this recipe only
# copies and converts those, so it runs without Xcode, as on CI.

# Generate all app icon formats from the compiled app icon via `tauri icon` (incremental)
asset-app-icons:
    #!/usr/bin/env bash
    set -euo pipefail
    compiled="assets/app-icon/compiled"
    icons="src-tauri/icons"
    source_icon="$icons/app-icon-source.png"

    # The recipe runs in three stages: take the 1024 px Default render
    # of the app icon as the source, invoke `tauri icon` to fan it out
    # into every platform-specific format, then put the macOS files
    # `actool` made in place of Tauri's. Each stage is checked
    # independently so a partial failure re-runs only what is missing,
    # rather than locking in a half-populated icons/ directory.

    # Stage 1: (re)build the source icon when missing or older than the
    # render. 8-bit RGBA is forced because Tauri rejects 16-bit PNGs.
    if [ ! -f "$source_icon" ] || [ "$compiled/app-icon-1024.png" -nt "$source_icon" ]; then
        echo "Converting app icon source..."
        magick "$compiled/app-icon-1024.png" -depth 8 -define png:color-type=6 "$source_icon"
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
        # `bun run` uses the project's `@tauri-apps/cli` through the
        # `tauri` script in package.json. `bunx tauri` would fetch Tauri's
        # legacy pre-1.0 `tauri` package from npm when node_modules
        # is missing.
        bun run tauri icon "$source_icon"
    else
        echo "App icons: up to date."
    fi

    # Stage 3: macOS takes the icon from `Assets.car`, named by
    # `CFBundleIconName` in src-tauri/Info.plist, with all macOS 26
    # appearances; `icon.icns`, `CFBundleIconFile`, is the one `actool`
    # made alongside it instead of Tauri's. tauri.conf.json copies
    # `Assets.car` into the bundle's Resources.
    for file in Assets.car:Assets.car AppIcon.icns:icon.icns; do
        from="$compiled/${file%%:*}"
        to="$icons/${file##*:}"
        if ! cmp -s "$from" "$to"; then
            cp "$from" "$to"
        fi
    done

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

# The candidate that is the app icon. Changing it needs
# `app-icon-compile` and a commit of `assets/app-icon/compiled/`.
app_icon := "aurora-rise"

# `actool` compiles the Icon Composer document into the asset catalog
# macOS 26 reads (all appearances) and an `.icns` for the bundle's
# `CFBundleIconFile`; its deployment target is the bundle's minimum
# system version. `ictool` renders the Default appearance that
# `asset-app-icons` turns into the icons of the other platforms.

# Compile the app icon candidate into assets/app-icon/compiled/ (needs Xcode with Icon Composer)
app-icon-compile:
    #!/usr/bin/env bash
    set -euo pipefail
    ictool="${ICTOOL:-/Applications/Xcode.app/Contents/Applications/Icon Composer.app/Contents/Executables/ictool}"
    document="assets/app-icon/{{ app_icon }}/{{ app_icon }}.icon"
    out="assets/app-icon/compiled"
    work=$(mktemp -d)
    trap 'rm -rf "$work"' EXIT

    # `--app-icon` names the icon inside the catalog; the document is
    # copied under that name so the two agree.
    cp -R "$document" "$work/AppIcon.icon"
    xcrun actool "$work/AppIcon.icon" --compile "$work" --app-icon AppIcon --include-all-app-icons \
        --enable-on-demand-resources NO --development-region en --target-device mac \
        --minimum-deployment-target 10.13 --platform macosx \
        --output-partial-info-plist "$work/partial.plist" --errors --output-format human-readable-text
    mkdir -p "$out"
    cp "$work/Assets.car" "$work/AppIcon.icns" "$out/"
    # ictool writes 16 bits per channel; the 8-bit copy is a third of
    # the size and all the platform icons need.
    "$ictool" "$document" --export-image --output-file "$work/render.png" --platform macOS \
        --rendition Default --width 1024 --height 1024 --scale 1
    magick "$work/render.png" -depth 8 -define png:color-type=6 "$out/app-icon-1024.png"
    oxipng -o max --strip safe --quiet "$out/app-icon-1024.png"
    echo "Compiled {{ app_icon }} into $out."

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
