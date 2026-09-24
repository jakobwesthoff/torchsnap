# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file compiles all project artifacts. Torchsnap is a Tauri
# desktop app with a Vite frontend, Rust backend, and WASM gadgets.

# `stage-bundled-gadgets` rebuilds and stages only the whitelisted
# gadgets from `gadgets/bundled.toml` so Tauri's bundler picks them up
# as resources. `build-gadgets` is still run afterwards so every gadget
# under `gadgets/` — bundled or not — has an up-to-date committed
# `.torchsnap` for the dev-directory loader.

# `asset-mascot-data` produces `src/derived/mascots.json`, which the
# frontend imports directly. It is generated rather than tracked, so
# every recipe that compiles the frontend depends on it — that also
# keeps the mascot trim values in step with the source PNGs whenever a
# new mascot is added.

# Tauri code signs the macOS bundle whenever `APPLE_SIGNING_IDENTITY`
# is set, and then enables the hardened runtime with the entitlements
# from `src-tauri/Entitlements.plist` (ADR 0046). `--sign` defaults the
# identity to `-` (ad-hoc) and keeps any identity already set in the
# environment, such as a Developer ID. Without `--sign` the bundle stays
# unsigned unless the environment sets an identity, so lldb can still
# attach to local builds (ADR 0047).

# Build the Tauri app (gadgets + frontend + Rust)
[arg('profile', long='release', value='release')]
[arg('sign', long='sign', value='sign')]
build profile="debug" sign="": asset-mascot-data
    just stage-bundled-gadgets
    just build-gadgets
    {{ if sign == "sign" { "APPLE_SIGNING_IDENTITY=\"${APPLE_SIGNING_IDENTITY:--}\"" } else { "" } }} bun run tauri build {{ if profile == "release" { "" } else { "--debug" } }}
    {{ if sign == "sign" { "just notarize-dmg " + profile } else { "" } }}

# `fullcycle` never builds a bundle, so the `.torchsnap` registration
# in `src-tauri/Info.plist` and `bundle.fileAssociations` (ADR 52) is
# checked against a built one here. `release-build` runs the same check.

# Check that a built bundle registers `.torchsnap` files (macOS)
[macos]
verify-bundle profile="debug":
    tools/verify-bundle-plist "src-tauri/target/{{ profile }}/bundle/macos/$(jq -r .productName src-tauri/tauri.conf.json).app"

# With notarization credentials in the environment, Tauri notarizes the
# app and staples the ticket to it, but only signs the DMG around it.
# Gatekeeper rejects that DMG as "Unnotarized Developer ID", so this
# recipe submits the DMG as well and staples its ticket. It accepts the
# same variables as Tauri: an App Store Connect API key
# (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`) or an Apple
# ID (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`). Ad-hoc signed
# builds cannot be notarized, so it skips them, and it skips builds
# without credentials.

# Notarize and staple the DMG of a signed build
notarize-dmg profile="release":
    #!/usr/bin/env bash
    set -euo pipefail

    if [ "${APPLE_SIGNING_IDENTITY:--}" = "-" ]; then
        echo "Ad-hoc signed build, skipping DMG notarization."
        exit 0
    fi
    if [ -n "${APPLE_API_KEY:-}" ]; then
        credentials=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER")
    elif [ -n "${APPLE_ID:-}" ]; then
        credentials=(--apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID")
    else
        echo "No notarization credentials set, skipping DMG notarization."
        exit 0
    fi

    # Tauri names the DMG `<productName>_<version>_<arch>.dmg` and leaves
    # the DMGs of earlier builds in the same directory, so only those of
    # the configured name and version are submitted.
    name=$(jq -r .productName src-tauri/tauri.conf.json)
    version=$(jq -r .version src-tauri/tauri.conf.json)
    shopt -s nullglob
    dmgs=(src-tauri/target/{{ profile }}/bundle/dmg/"${name}_${version}"_*.dmg)
    if [ ${#dmgs[@]} -eq 0 ]; then
        echo "No ${name}_${version}_*.dmg found in src-tauri/target/{{ profile }}/bundle/dmg/." >&2
        exit 1
    fi

    for dmg in "${dmgs[@]}"; do
        echo "Notarizing $dmg"
        # Only an "Accepted" submission has a ticket that stapler can
        # attach, so that status decides, whatever the exit code says.
        result=$(xcrun notarytool submit "$dmg" "${credentials[@]}" --wait --output-format json)
        echo "$result"
        if ! grep -q '"status":"Accepted"' <<<"$result"; then
            echo "Notarization of $dmg was not accepted." >&2
            exit 1
        fi
        xcrun stapler staple "$dmg"
    done

# Build only the frontend (Vite + TypeScript)
build-frontend: asset-mascot-data
    bun run build

# Check the Rust crate compiles
build-check:
    cargo check --manifest-path src-tauri/Cargo.toml
