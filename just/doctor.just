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

    echo "Required:"
    check cargo       "Rust toolchain"              "https://rustup.rs"
    check bun         "JS/TS runtime & bundler"     "https://bun.sh"

    echo ""
    echo "Optional:"
    check shellcheck  "shell script linting"        "brew install shellcheck"

    echo ""
    bg_green=$'\033[42;30m' bg_red=$'\033[41;37m'
    echo ""
    if [ "$missing" -eq 0 ]; then
        printf "${bg_green} All %d tools found. ${reset}\n" "$ok"
    else
        printf "${bg_red} %d found, %d missing. ${reset}\n" "$ok" "$missing"
        exit 1
    fi
