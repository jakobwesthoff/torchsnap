# How to test Torchsnap on a Raspberry Pi

The Linux testbed is a Raspberry Pi that the `just testbed-*` recipes
drive from the Mac over SSH. This guide is the counterpart to
[How to build Torchsnap on Fedora 43](Howto-build-on-fedora-43.md): that
one sets up a Linux machine to develop on, this one sets up a machine
that recipes on the Mac work with.

## 1. Requirements

- A Raspberry Pi 5 with Raspberry Pi OS 13 (trixie, arm64), the system
  the testbed was built on.
- SSH login to the Pi with a key, without a password prompt.
- `sudo` without a password for the login user on the Pi, since the
  provision script installs packages and changes system files.
- `rsync` on the Mac and on the Pi.
- The environment variable `TORCHSNAP_LINUX_HOST` on the Mac, set to the
  SSH target, for example:

  ```sh
  export TORCHSNAP_LINUX_HOST=user@pi.local
  ```

  The repository does not name a machine. Every recipe that talks to the
  Pi stops with `TORCHSNAP_LINUX_HOST is not set` when the variable is
  missing or empty.

## 2. Syncing the working tree

```sh
just testbed-sync
```

mirrors the working tree to `~/torchsnap` on the Pi, including changes
that are not committed yet. Files that `.gitignore` names, such as
`target/` and `node_modules/`, and the `.git` directory stay on the Mac.
Files deleted on the Mac are deleted on the Pi; build output on the Pi
survives a sync.

## 3. Provisioning

```sh
just testbed-provision
just testbed-provision --monitor HDMI-1:90:2   # also set a monitor's layout
```

syncs the working tree and runs `tools/linux-testbed/provision/provision`
on the Pi as the login user, which needs `sudo` without a password
there. Every step checks first and changes only what is missing, so a
second run reports `provision: 0 changed, …`. Each step prints `ok`,
`changed` or `failed` with its name; a failed step stops the run with
exit status 1.

Some steps talk to the running GNOME session. While the Pi still runs
another desktop, as a fresh Raspberry Pi OS does, they print `deferred`
and the script exits with status 3. `just testbed-provision` then reboots
the Pi into GNOME (gdm logs the user in), waits until the session is
ready, and runs the script again.

`--monitor CONNECTOR:TRANSFORM:SCALE` stores one monitor's layout in the
GNOME session, for example `HDMI-1:90:2` for a monitor turned to
portrait at scale 2; `gdctl show` on the Pi lists the connectors.

### Desktop-independent steps

| Step | What it sets up |
|---|---|
| `base-packages` | apt packages: the libraries Tauri links against, `zip unzip imagemagick jq shellcheck curl` for `just install`, `wl-clipboard socat ffmpeg`, `gdb strace`, and `cmake scdoc git` to build ydotool |
| `rustup` | rustup in `~/.cargo` without a default toolchain; the first cargo call in the tree installs the release `rust-toolchain.toml` pins |
| `bun` | Bun 1.4.4 in `~/.bun` |
| `just` | just 1.58.0 in `~/.local/bin` |
| `wasm-tools` | wasm-tools 1.246.2, the version `just check-wit` and `just fmt-wit` run with on the Mac |
| `ydotool` | ydotool v1.0.4, built from source into `/usr/local` (Debian 13 has no package) |
| `uinput` | loads the `uinput` kernel module at boot and gives the `input` group access to `/dev/uinput`, which ydotool writes to |
| `ydotool-service` | ydotool's daemon as an enabled systemd user service |
| `sleep-masked` | masks `sleep`, `suspend`, `hibernate` and `hybrid-sleep`, so the Pi never suspends |

### GNOME steps

GNOME comes from Debian and is installed next to the Pi's own desktop;
lightdm and the labwc sessions stay installed.

| Step | What it sets up |
|---|---|
| `gnome-packages` | `task-gnome-desktop` (GNOME 48), the AppIndicator extension (installed, disabled) and `gstreamer1.0-pipewire`. The install keeps the current display manager; the `gdm` step switches. |
| `testbed-extension` | the testbed's GNOME Shell extension in `/usr/local/share/gnome-shell/extensions/` (see below) |
| `dconf-defaults` | system-wide settings defaults in `/etc/dconf/db/local.d/50-torchsnap-testbed`: no screen blanking, no lock screen, no suspend on idle, no welcome tour, the testbed extension enabled, flat mouse acceleration |
| `gdm` | gdm as the display manager from the next boot, with automatic login for the provisioning user |
| `accountsservice-session` | GNOME as the session gdm starts for that user |
| `user-settings` | the testbed extension in the user's own list of enabled extensions (switching the AppIndicator extension writes that list), and the `us` keyboard layout after the Pi's own layouts |
| `keyring` | an unencrypted keyring named `testbed` as the default keyring (see below) |
| `rdp` | remote desktop over RDP, see section 5 |

Things to know about these:

- **Shell extension.** GNOME Shell answers the D-Bus methods the testbed
  uses for screenshots, screen recording and reading window state
  (`Eval`, `Screenshot`, `Screencast`) only for a few of its own
  programs, unless it runs in "unsafe mode". GNOME Shell 48 has no
  switch for that mode, so the testbed's extension turns it on. Every
  local process can then call these methods. GNOME Shell announces the
  switch with a notification whose banner covers the top of the screen;
  the extension removes that notification again. It also closes the
  overview GNOME opens after login, which otherwise takes every key
  typed.
- **Keyring.** With automatic login, nobody types the password that
  unlocks the login keyring, and gnome-remote-desktop cannot read its
  credentials from a locked keyring. The `testbed` keyring has no
  password: it is stored unencrypted in
  `~/.local/share/keyrings/testbed.keyring` and holds the RDP password.
  It is locked again after each login and unlocks without a prompt on
  first use.
- **Unattended.** Screen lock, blanking and suspend are off, and the Pi
  logs in by itself after every boot.

The provision script has not been run on a freshly installed system: it
was checked on a Pi first set up by hand with the same commands, by
running it there, by undoing single steps and running it again, and by
rebooting (self-test items 1, U and R).

A command run over SSH does not get the `PATH` entries these installers
add to the shell profile, so the provision script puts `~/.cargo/bin`,
`~/.bun/bin` and `~/.local/bin` on `PATH` itself.

## 4. Rebooting and running commands

```sh
just testbed-reboot
```

reboots the Pi and returns once it is back with a new boot id and its
GNOME session takes input: GNOME Shell has finished starting and the
overview is closed. It gives up after 300 seconds
(`TORCHSNAP_LINUX_REBOOT_TIMEOUT`). The reboot goes over plain SSH, not
through `TORCHSNAP_LINUX_RUNNER`.

```sh
just testbed-exec 'systemctl --user status ydotool.service'
```

runs one shell command line on the Pi.

## 5. Building and running the app

```sh
just testbed-build
```

syncs the working tree, then runs `just install` and
`just build --config '{"bundle":{"active":false}}'` on the Pi: all
gadgets and a debug app, without bundling, at
`src-tauri/target/debug/torchsnap` (788 MB). Measured on a Raspberry Pi
5: the first build took 96 s for `just install` and 802 s for the build;
after that a build takes 102 to 113 s, whether nothing, one Rust line or
one frontend line changed. Each build rebuilds the frontend and then
compiles the `torchsnap` crate again (about 54 s) and links the binary
(about 32 s).

```sh
just testbed-app-start
just testbed-control show
just testbed-control query '{"text": "calc"}'
just testbed-control status
just testbed-app-log 50
just testbed-app-stop
```

`testbed-app-start` runs the debug app in the Pi's GNOME session as the
systemd user unit `torchsnap-testbed-app`, with its output in
`~/.cache/torchsnap-testbed/app.log`, and returns once the app's control
socket answers. Before the start it sets two keys in the app's real
settings file, `~/.local/share/app.torchsnap/settings.json`, and leaves
every other key as it is: `controlChannel.enabled` to `true`, so the
control socket is open, and `welcome.seenRevision`, so the welcome window
does not open. The welcome is where the app asks about automatic update
checks, so that setting keeps its default.

`testbed-control METHOD [JSON]` sends one request to the control socket
([Control API](control-api.md)) and prints the response; it exits 1 when
the response is an error. Right after a start, `show` fails until the
launcher's frontend has reported its layout; `testbed-control show`
waits up to 60 s for that. Parameters go in quotes as one argument.

## 6. Remote viewing over RDP

The `rdp` step turns on GNOME's remote desktop for the running session:
an RDP client sees and controls the Pi's screen. Connect to the Pi on
port 3389 as user `testbed`; the password is in
`~/.config/torchsnap-testbed/rdp-password` on the Pi, generated at the
first provisioning. The TLS certificate is self-signed and lives in
`~/.local/share/gnome-remote-desktop/` on the Pi.

## 7. Self-test

```sh
just testbed-selftest        # the default items
just testbed-selftest 1      # only item 1
```

runs the testbed's checks against the Pi and prints `PASS item N` or
`FAIL item N: …` per item; it exits 1 if any item failed and 2 for an
unknown item.

| Item | Checks |
|---|---|
| 1 | Provisioning twice: the second run exits 0 and reports `0 changed`. |
| 2 | Only when named. Sync and build: the build exits 0 and leaves a debug binary written after the build started. |
| R | Only when named. Reboots the Pi, then checks that it is back in GNOME with ydotool's daemon running, `/dev/uinput` usable by the `input` group, RDP listening, the `testbed` keyring unlocking without a prompt, no unsafe-mode notification in GNOME Shell's message tray, and provisioning reporting `0 changed`. |
| U | Only when named. Undoes one provision step at a time (the extension, the settings defaults, gdm, the AccountsService session, uinput, ydotool's service, a sleep mask, the user settings, the keyring, RDP) and checks that provisioning then reports `changed` for exactly that step. |

## A visible terminal on the Pi

`TORCHSNAP_LINUX_RUNNER`, when set, is put in front of every command the
recipes run on the Pi, in place of `bash -c`. A tool that runs commands
in a terminal on the Pi's screen and passes their output and exit status
back lets someone at the Pi watch the testbed work, for example:

```sh
export TORCHSNAP_LINUX_RUNNER='~/.local/bin/testbed-run --'
```

The command arrives as one quoted argument; the runner has to join and
evaluate its arguments, as `bash -c` would. A runner that answers with
exit status 75 is reported as having refused the command because it was
busy.
