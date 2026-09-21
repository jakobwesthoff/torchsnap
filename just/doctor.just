# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# This file provides an environment health check that verifies all
# required and optional external tools are installed.

# Check that all required external tools are installed
doctor:
    #!/usr/bin/env bash
    set -euo pipefail
    missing=0
    ok=0
    green=$'\033[32m' red=$'\033[31m' reset=$'\033[0m'

    check() {
        local cmd="$1" purpose="$2" install="$3"
        if command -v "$cmd" &>/dev/null; then
            version=""
            for flag in --version -version -V; do
                candidate=$("$cmd" $flag 2>&1 | head -1 || true)
                case "$candidate" in
                    *[Uu]sage*|*[Uu]nknown*|*[Ii]nvalid*|"") continue ;;
                esac
                version="$candidate"
                break
            done
            if [ -n "$version" ]; then
                printf "  ${green}✓${reset}  %-14s %s\n" "$cmd" "$version"
            else
                printf "  ${green}✓${reset}  %-14s installed\n" "$cmd"
            fi
            ok=$((ok + 1))
        else
            printf "  ${red}✗${reset}  %-14s missing — %s\n" "$cmd" "$purpose"
            [ -n "$install" ] && printf "     %-14s install: %s\n" "" "$install"
            missing=$((missing + 1))
        fi
    }

    # `rust-toolchain.toml` pins the Rust release, and only rustup
    # reads it. A cargo installed any other way ignores the pin.
    echo "Required:"
    check rustup      "installs the Rust release pinned in rust-toolchain.toml" "https://rustup.rs"
    check cargo       "Rust toolchain"              "https://rustup.rs"
    check bun         "JS/TS runtime & bundler"     "https://bun.sh"

    # The lockfiles are maintained with Bun 1.4, whose `bun update`
    # also refreshes transitive packages. Bun has no per-project
    # version switch, so the minimum is checked here.
    min_bun="1.4.0"
    if command -v bun &>/dev/null; then
        bun_version=$(bun --version)
        if [ "$(printf '%s\n' "$min_bun" "$bun_version" | sort -V | head -1)" != "$min_bun" ]; then
            printf "  ${red}✗${reset}  %-14s %s is older than %s\n" "bun" "$bun_version" "$min_bun"
            printf "     %-14s upgrade: %s\n" "" "bun upgrade"
            missing=$((missing + 1))
        fi
    fi

    echo ""
    echo "WASM gadget toolchain:"
    check zip             "create .torchsnap gadget archives" "brew install zip"
    check wasm-tools      "inspect/validate WASM components"  "cargo install wasm-tools"

    # Check that the wasm32-wasip2 target is installed.
    if rustup target list --installed 2>/dev/null | grep -q wasm32-wasip2; then
        printf "  ${green}✓${reset}  %-14s installed\n" "wasm32-wasip2"
        ok=$((ok + 1))
    else
        printf "  ${red}✗${reset}  %-14s missing — Rust compilation target for WASM gadgets\n" "wasm32-wasip2"
        printf "     %-14s install: %s\n" "" "rustup target add wasm32-wasip2"
        missing=$((missing + 1))
    fi

    echo ""
    # `just install` runs the whole asset pipeline, and
    # `asset-mascot-data` (jq) also runs before `build`, `start` and
    # `check-types`.
    echo "Asset pipeline:"
    check magick      "image resizing (ImageMagick 7)" "brew install imagemagick"
    check cwebp       "WebP conversion"                "brew install webp"
    check jq          "merge mascot trim data into mascots.json" "brew install jq"
    check curl        "download the DuckDuckGo bang database"    "brew install curl"

    echo ""
    echo "Optional:"
    check shellcheck  "shell script linting"        "brew install shellcheck"
    check uv          "run the tools/detect-mascot-* scripts" "brew install uv"
    check cargo-audit "check Cargo lockfiles in \`just audit\`" "cargo install cargo-audit --locked"

    echo ""
    bg_green=$'\033[42;30m' bg_red=$'\033[41;37m'
    echo ""
    if [ "$missing" -eq 0 ]; then
        printf "${bg_green} All %d tools found. ${reset}\n" "$ok"
    else
        printf "${bg_red} %d found, %d missing. ${reset}\n" "$ok" "$missing"
        exit 1
    fi
