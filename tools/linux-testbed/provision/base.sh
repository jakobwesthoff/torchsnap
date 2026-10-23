# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# shellcheck shell=bash

# Desktop-independent provision steps: build tools, the Rust, JS and WASM
# toolchains, ydotool for input, and no suspend. Sourced by `provision`,
# which defines `provision_dir` and `tree_dir`.

# =========================================================
# apt packages
# =========================================================

base_packages=(
    # Libraries Tauri's Rust crates link against.
    build-essential pkg-config libssl-dev
    libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev
    libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev
    libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev
    # The asset pipeline of `just install`.
    zip unzip imagemagick jq shellcheck curl
    # Driving and observing the app.
    wl-clipboard socat ffmpeg
    # Debugging.
    gdb strace
    # Building ydotool.
    cmake scdoc git
)

packages_installed() {
    local package
    for package in "$@"; do
        [ "$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null)" = "install ok installed" ] || return 1
    done
}

check_base_packages() {
    packages_installed "${base_packages[@]}"
}

apply_base_packages() {
    sudo apt-get update
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y "${base_packages[@]}"
}

# =========================================================
# Toolchains in the user's home
# =========================================================

# rustup without a default toolchain: the first cargo call inside the
# tree installs the release `rust-toolchain.toml` pins.
check_rustup() {
    [ -x "$HOME/.cargo/bin/rustup" ]
}

apply_rustup() {
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain none --profile minimal
}

readonly bun_version="1.4.4"

check_bun() {
    [ "$("$HOME/.bun/bin/bun" --version 2>/dev/null)" = "$bun_version" ]
}

apply_bun() {
    curl -fsSL https://bun.sh/install | bash -s "bun-v$bun_version"
}

readonly just_version="1.58.0"

check_just() {
    [ "$("$HOME/.local/bin/just" --version 2>/dev/null)" = "just $just_version" ]
}

apply_just() {
    mkdir -p "$HOME/.local/bin"
    curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
        | bash -s -- --tag "$just_version" --to "$HOME/.local/bin" --force
}

# `just check-wit` and `just fmt-wit` output can differ between
# wasm-tools versions, so the machine uses the version the Mac uses.
readonly wasm_tools_version="1.246.2"

check_wasm_tools() {
    case "$("$HOME/.cargo/bin/wasm-tools" --version 2>/dev/null)" in
        "wasm-tools $wasm_tools_version"*) return 0 ;;
        *) return 1 ;;
    esac
}

apply_wasm_tools() {
    # Inside the tree, so cargo runs on the pinned toolchain.
    (cd "$tree_dir" && cargo install wasm-tools --version "$wasm_tools_version" --locked --force)
}

# =========================================================
# ydotool
# =========================================================

# Debian 13 has no ydotool package. GNOME offers no Wayland protocol for
# virtual keyboards or pointers, so input goes through the kernel's
# uinput device, which ydotool's daemon writes to.
readonly ydotool_tag="v1.0.4"
readonly ydotool_stamp="/usr/local/share/torchsnap-testbed/ydotool-$ydotool_tag"

check_ydotool() {
    [ -x /usr/local/bin/ydotoold ] && [ -x /usr/local/bin/ydotool ] && [ -f "$ydotool_stamp" ]
}

apply_ydotool() {
    local src="$HOME/.cache/torchsnap-testbed/ydotool-src"
    rm -rf "$src"
    mkdir -p "$(dirname "$src")"
    git clone --depth 1 --branch "$ydotool_tag" https://github.com/ReimuNotMoe/ydotool "$src"
    cmake -S "$src" -B "$src/build" -DCMAKE_BUILD_TYPE=Release
    cmake --build "$src/build" -j4
    sudo cmake --install "$src/build"
    sudo mkdir -p "$(dirname "$ydotool_stamp")"
    sudo touch "$ydotool_stamp"
}

# The uinput module is not loaded at boot by default, and its device node
# belongs to root alone. The rule gives the `input` group access, which
# the login user is in on Raspberry Pi OS.
readonly uinput_rule="/etc/udev/rules.d/60-torchsnap-testbed-uinput.rules"
readonly uinput_module="/etc/modules-load.d/torchsnap-testbed-uinput.conf"

check_uinput() {
    cmp -s "$provision_dir/base/60-torchsnap-testbed-uinput.rules" "$uinput_rule" \
        && cmp -s "$provision_dir/base/torchsnap-testbed-uinput.conf" "$uinput_module" \
        && [ "$(stat -c '%G %a' /dev/uinput 2>/dev/null)" = "input 660" ]
}

apply_uinput() {
    sudo install -m 0644 "$provision_dir/base/60-torchsnap-testbed-uinput.rules" "$uinput_rule"
    sudo install -m 0644 "$provision_dir/base/torchsnap-testbed-uinput.conf" "$uinput_module"
    sudo udevadm control --reload
    sudo modprobe uinput
    # When the module was already loaded, its device exists from before
    # the rule; replaying the add event applies the rule to it.
    sudo udevadm trigger --action=add --subsystem-match=misc --sysname-match=uinput
    sudo udevadm settle
}

check_ydotool_service() {
    systemctl --user is-enabled --quiet ydotool.service \
        && systemctl --user is-active --quiet ydotool.service
}

apply_ydotool_service() {
    systemctl --user daemon-reload
    systemctl --user reset-failed ydotool.service 2>/dev/null || true
    systemctl --user enable --now ydotool.service
    systemctl --user restart ydotool.service
}

# =========================================================
# Suspend
# =========================================================

# The machine runs unattended; a suspended machine answers nothing.
readonly sleep_targets=(sleep.target suspend.target hibernate.target hybrid-sleep.target)

check_sleep_masked() {
    local target
    for target in "${sleep_targets[@]}"; do
        [ "$(systemctl is-enabled "$target" 2>/dev/null)" = "masked" ] || return 1
    done
}

apply_sleep_masked() {
    sudo systemctl mask "${sleep_targets[@]}"
}

# =========================================================
# Order
# =========================================================

base_steps() {
    step base-packages check_base_packages apply_base_packages
    step rustup check_rustup apply_rustup
    step bun check_bun apply_bun
    step just check_just apply_just
    step wasm-tools check_wasm_tools apply_wasm_tools
    step ydotool check_ydotool apply_ydotool
    step uinput check_uinput apply_uinput
    step ydotool-service check_ydotool_service apply_ydotool_service
    step sleep-masked check_sleep_masked apply_sleep_masked
}
