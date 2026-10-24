# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.
# shellcheck shell=bash
# shellcheck disable=SC2154 # provision_dir is set by provision, which sources this file.

# GNOME provision steps: Debian's GNOME next to the existing desktops, gdm
# with autologin, an unattended session (no lock, blanking or welcome
# tour), the testbed's GNOME Shell extension, and remote viewing over
# RDP. Sourced by `provision`, which defines `provision_dir`, `tree_dir`
# and `monitor_spec`.

readonly gnome_dir="$provision_dir/gnome"

# The steps marked `session` talk to the running GNOME session over the
# user's D-Bus session bus.
session_running() {
    gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus \
        --method org.freedesktop.DBus.NameHasOwner org.gnome.Shell 2>/dev/null \
        | grep -q '^(true,)$'
}

# =========================================================
# Packages
# =========================================================

gnome_packages=(task-gnome-desktop gnome-shell-extension-appindicator gstreamer1.0-pipewire)

check_gnome_packages() {
    packages_installed "${gnome_packages[@]}"
}

# Installing gdm3 asks which display manager starts at boot. Answering
# with the current one keeps the running desktop untouched during the
# install; the gdm step switches at the next boot.
apply_gnome_packages() {
    local current
    current="$(basename "$(cat /etc/X11/default-display-manager 2>/dev/null || echo lightdm)")"
    echo "gdm3 shared/default-x-display-manager select $current" | sudo debconf-set-selections
    echo "$current shared/default-x-display-manager select $current" | sudo debconf-set-selections
    sudo apt-get update
    sudo DEBIAN_FRONTEND=noninteractive apt-get install -y "${gnome_packages[@]}"
}

# =========================================================
# The testbed's GNOME Shell extension
# =========================================================

readonly extension_uuid="unsafe-mode@testbed.torchsnap.app"
# A system-wide place, so a headless gnome-shell with its own data
# directory finds the extension too.
readonly extension_dir="/usr/local/share/gnome-shell/extensions/$extension_uuid"
# GNOME Shell prefers an extension in the user's data directory over the
# system-wide one with the same uuid.
readonly extension_user_dir="$HOME/.local/share/gnome-shell/extensions/$extension_uuid"

check_extension() {
    cmp -s "$gnome_dir/$extension_uuid/metadata.json" "$extension_dir/metadata.json" \
        && cmp -s "$gnome_dir/$extension_uuid/extension.js" "$extension_dir/extension.js" \
        && [ ! -e "$extension_user_dir" ]
}

apply_extension() {
    sudo install -d "$extension_dir"
    sudo install -m 0644 "$gnome_dir/$extension_uuid/metadata.json" "$gnome_dir/$extension_uuid/extension.js" "$extension_dir/"
    rm -rf "$extension_user_dir"
}

# =========================================================
# Settings defaults
# =========================================================

# System-wide defaults through the dconf `local` database, which the
# user profile of Raspberry Pi OS already reads. They apply before the
# first login, so the first session starts unlocked and without the tour.
readonly dconf_defaults="/etc/dconf/db/local.d/50-torchsnap-testbed"

check_dconf_defaults() {
    cmp -s "$gnome_dir/50-torchsnap-testbed" "$dconf_defaults" \
        && [ /etc/dconf/db/local -nt "$dconf_defaults" ]
}

apply_dconf_defaults() {
    sudo install -m 0644 "$gnome_dir/50-torchsnap-testbed" "$dconf_defaults"
    sudo dconf update
}

# A value the user set shadows the default above. Switching the
# AppIndicator extension writes the user's extension list, so that list
# has to keep the testbed extension. The `us` layout joins the machine's
# own layouts: the testbed types US key codes and switches to `us` for
# its own typing.
check_user_settings() {
    gsettings get org.gnome.shell enabled-extensions | grep -qF "'$extension_uuid'" \
        && gsettings get org.gnome.desktop.input-sources sources | grep -qF "('xkb', 'us')"
}

apply_user_settings() {
    local extensions sources
    extensions="$(gsettings get org.gnome.shell enabled-extensions)"
    if ! grep -qF "'$extension_uuid'" <<<"$extensions"; then
        gsettings set org.gnome.shell enabled-extensions "$(append_to_gvariant_list "$extensions" "'$extension_uuid'")"
    fi
    sources="$(gsettings get org.gnome.desktop.input-sources sources)"
    if ! grep -qF "('xkb', 'us')" <<<"$sources"; then
        gsettings set org.gnome.desktop.input-sources sources "$(append_to_gvariant_list "$sources" "('xkb', 'us')")"
    fi
}

# append_to_gvariant_list LIST ITEM: `@as []` or `[]` -> `[ITEM]`,
# `[a, b]` -> `[a, b, ITEM]`.
append_to_gvariant_list() {
    local list="$1"
    local item="$2"
    list="${list#@* }"
    if [ "$list" = "[]" ]; then
        printf '[%s]' "$item"
    else
        printf '%s, %s]' "${list%]}" "$item"
    fi
}

# =========================================================
# Display manager and autologin
# =========================================================

readonly gdm_config="/etc/gdm3/daemon.conf"
readonly display_manager_unit="/etc/systemd/system/display-manager.service"

gdm_config_wanted() {
    sed "s/@USER@/$USER/" "$gnome_dir/daemon.conf"
}

# gdm.service has no [Install] section on Debian, so `systemctl enable`
# cannot make it the display manager; the unit alias and the
# default-display-manager file are what the gdm3 package's own setup
# writes. The switch takes effect at the next boot; the running desktop
# stays as it is. lightdm stays installed for the labwc session.
check_gdm() {
    gdm_config_wanted | cmp -s - "$gdm_config" \
        && [ "$(cat /etc/X11/default-display-manager 2>/dev/null)" = "/usr/sbin/gdm3" ] \
        && [ "$(readlink -f "$display_manager_unit")" = "/usr/lib/systemd/system/gdm.service" ]
}

apply_gdm() {
    gdm_config_wanted | sudo tee "$gdm_config" >/dev/null
    echo /usr/sbin/gdm3 | sudo tee /etc/X11/default-display-manager >/dev/null
    sudo ln -sf /usr/lib/systemd/system/gdm.service "$display_manager_unit"
    sudo systemctl daemon-reload
}

# gdm logs the user into the session AccountsService remembers for them.
# The file is readable by root only.
accountsservice_file() {
    echo "/var/lib/AccountsService/users/$USER"
}

check_accountsservice_session() {
    sudo grep -qx 'Session=gnome' "$(accountsservice_file)"
}

apply_accountsservice_session() {
    set_accountsservice_session "$(accountsservice_file)"
}

# set_accountsservice_session FILE: sets `Session=gnome` in the `[User]`
# group and keeps every other line, including `XSession`, which lightdm
# reads. A file it creates is readable by root only, as the existing
# ones are; an existing file keeps its mode.
set_accountsservice_session() {
    local file="$1"
    if ! sudo test -f "$file"; then
        sudo install -m 0600 /dev/null "$file"
        printf '[User]\nSession=gnome\n' | sudo tee -a "$file" >/dev/null
    elif sudo grep -q '^Session=' "$file"; then
        sudo sed -i.bak 's/^Session=.*/Session=gnome/' "$file"
        sudo rm -f "$file.bak"
    elif sudo grep -qx '\[User\]' "$file"; then
        sudo sed -i.bak '/^\[User\]$/a\
Session=gnome
' "$file"
        sudo rm -f "$file.bak"
    else
        printf '[User]\nSession=gnome\n' | sudo tee -a "$file" >/dev/null
    fi
}

# =========================================================
# Keyring
# =========================================================

# gnome-remote-desktop keeps its RDP credentials in the Secret Service.
# With autologin nobody types a password, so the login keyring stays
# locked and gnome-remote-desktop cannot read them. A keyring without a
# password is stored unencrypted and unlocks without a prompt; it becomes
# the default keyring, where the credentials go.
readonly keyring_path="/org/freedesktop/secrets/collection/testbed"
# The default keyring before the step changed it, for undoing the step:
# a collection's object path, or "/" for none.
readonly previous_default_keyring="$HOME/.config/torchsnap-testbed/previous-default-keyring"

secret_service() {
    busctl --user call org.freedesktop.secrets /org/freedesktop/secrets org.freedesktop.Secret.Service "$@"
}

# Whether gnome-keyring has read the testbed keyring's file.
keyring_loaded() {
    local collections
    collections="$(busctl --user get-property org.freedesktop.secrets /org/freedesktop/secrets org.freedesktop.Secret.Service Collections)" \
        && grep -qF "\"$keyring_path\"" <<<"$collections"
}

# The keyring is locked again after every login. What counts is that it
# is the default and unlocks without a prompt: an Unlock call that needs
# no prompt answers with the prompt path "/", which is what
# gnome-remote-desktop's own lookup relies on.
check_keyring() {
    [ "$(secret_service ReadAlias s default 2>/dev/null)" = "o \"$keyring_path\"" ] \
        && [ "$(secret_service Unlock ao 1 "$keyring_path" 2>/dev/null)" = "aoo 1 \"$keyring_path\" \"/\"" ]
}

apply_keyring() {
    local file="$HOME/.local/share/keyrings/testbed.keyring"
    if [ ! -f "$file" ]; then
        install -d -m 0700 "$(dirname "$file")"
        install -m 0600 "$gnome_dir/testbed.keyring" "$file"
    fi
    # gnome-keyring reads new keyring files only when it starts.
    if ! keyring_loaded; then
        systemctl --user restart gnome-keyring-daemon.service
        wait_for 10 1 keyring_loaded
    fi
    secret_service Unlock ao 1 "$keyring_path" >/dev/null
    # ReadAlias answers `o "PATH"`.
    local previous
    previous="$(secret_service ReadAlias s default)"
    previous="${previous#o \"}"
    previous="${previous%\"}"
    if [ "$previous" != "$keyring_path" ]; then
        install -d -m 0700 "$(dirname "$previous_default_keyring")"
        echo "$previous" >"$previous_default_keyring"
    fi
    secret_service SetAlias so default "$keyring_path"
}

# =========================================================
# Remote viewing over RDP
# =========================================================

readonly rdp_password_file="$HOME/.config/torchsnap-testbed/rdp-password"
readonly rdp_dir="$HOME/.local/share/gnome-remote-desktop"

rdp_status() {
    grdctl status 2>/dev/null
}

check_rdp() {
    local status
    status="$(rdp_status)"
    [ -s "$rdp_password_file" ] \
        && grep -q 'Status: enabled' <<<"$status" \
        && grep -q 'View-only: no' <<<"$status" \
        && grep -qF "TLS certificate: $rdp_dir/rdp-tls.crt" <<<"$status" \
        && grep -q 'Username: (hidden)' <<<"$status" \
        && systemctl --user is-enabled --quiet gnome-remote-desktop.service \
        && systemctl --user is-active --quiet gnome-remote-desktop.service
}

apply_rdp() {
    if [ ! -s "$rdp_password_file" ]; then
        install -d -m 0700 "$(dirname "$rdp_password_file")"
        (umask 077 && openssl rand -base64 18 | tr -d '/+=\n' >"$rdp_password_file")
    fi
    if [ ! -f "$rdp_dir/rdp-tls.key" ] || [ ! -f "$rdp_dir/rdp-tls.crt" ]; then
        install -d -m 0700 "$rdp_dir"
        openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -subj /CN=torchsnap-testbed \
            -keyout "$rdp_dir/rdp-tls.key" -out "$rdp_dir/rdp-tls.crt" 2>/dev/null
    fi
    grdctl rdp set-tls-cert "$rdp_dir/rdp-tls.crt"
    grdctl rdp set-tls-key "$rdp_dir/rdp-tls.key"
    grdctl rdp set-credentials testbed "$(cat "$rdp_password_file")"
    grdctl rdp disable-view-only
    grdctl rdp enable
    systemctl --user enable --now gnome-remote-desktop.service
}

# =========================================================
# Monitor
# =========================================================

# --monitor CONNECTOR:TRANSFORM:SCALE. Mutter stores the layout in
# ~/.config/monitors.xml, so it survives reboots.
monitor_connector() { echo "${monitor_spec%%:*}"; }
monitor_transform() { local rest="${monitor_spec#*:}"; echo "${rest%%:*}"; }
monitor_scale() { echo "${monitor_spec##*:}"; }

# Prints "SCALE TRANSFORM" of the logical monitor that holds CONNECTOR,
# read from `gdctl show`.
gdctl_logical_monitor() {
    gdctl show | awk -v connector="$1" '
        /Logical monitor #/ { scale = ""; transform = "" }
        /Scale:/ { scale = $NF }
        /Transform:/ { transform = $NF }
        index($0, "──" connector " ") && scale != "" { print scale, transform; exit }
    '
}

check_monitor() {
    local wanted_scale
    wanted_scale="$(monitor_scale)"
    case "$wanted_scale" in *.*) ;; *) wanted_scale="$wanted_scale.0" ;; esac
    [ "$(gdctl_logical_monitor "$(monitor_connector)")" = "$wanted_scale $(monitor_transform)" ]
}

apply_monitor() {
    gdctl set --persistent --logical-monitor --primary \
        --scale "$(monitor_scale)" --transform "$(monitor_transform)" \
        --monitor "$(monitor_connector)"
}

# =========================================================
# Order
# =========================================================

desktop_steps() {
    step gnome-packages check_gnome_packages apply_gnome_packages
    step testbed-extension check_extension apply_extension
    step dconf-defaults check_dconf_defaults apply_dconf_defaults
    step gdm check_gdm apply_gdm
    step accountsservice-session check_accountsservice_session apply_accountsservice_session
    step user-settings check_user_settings apply_user_settings session
    step keyring check_keyring apply_keyring session
    step rdp check_rdp apply_rdp session
    if [ -n "$monitor_spec" ]; then
        step monitor check_monitor apply_monitor session
    fi
}
