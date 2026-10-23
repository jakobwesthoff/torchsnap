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
running it there, by undoing single steps or removing what they
installed and running it again, and by rebooting (self-test items 1, U,
P and R).

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

## 6. Observing the app

```sh
just testbed-screenshot launcher --window '^$'   # one window, by title or WM class
just testbed-screenshot top --area 0,0,1080,40   # an area, in logical pixels
just testbed-screenshot screen                    # the whole screen
just testbed-screenshot right --monitor 1         # one monitor
just testbed-launcher-state
just testbed-windows
just testbed-eval 'Main.overview.visible'
```

`testbed-screenshot NAME` takes the screenshot through GNOME Shell and
copies it to `target/linux-testbed/NAME.png` in the repository. Areas
and window rectangles are in logical pixels; the picture has the
screen's physical pixels, so on a monitor at scale 2 it is twice as
large.

`testbed-launcher-state` prints `{"visible", "focused", "monitor"}`:
`visible` comes from the app's control socket (`status`), `focused` and
`monitor` from GNOME Shell's view of the launcher window, the app's
window with an empty title. A hidden launcher has no window, so
`monitor` is `null` then. `testbed-windows` lists the app's windows
(WM class `torchsnap`) with title, rectangle, monitor and focus.
`testbed-eval JS` prints the value of a JavaScript expression evaluated
inside GNOME Shell, as JSON; it exits 1 when the expression throws.

## 7. Keyboard and pointer

```sh
just testbed-type --window '^torchsnap$' 'firefox'
just testbed-key --window '^torchsnap$' Escape
just testbed-key --shell-modal ctrl+a BackSpace
just testbed-pointer 540 960
just testbed-click 540 960 right
```

Input goes through ydotool, which writes to the kernel's uinput device,
so keys go to whatever has the keyboard: the app, a GNOME Shell dialog,
or a terminal on the Pi's screen. `testbed-type` and `testbed-key`
therefore name their target and refuse with exit status 4 when it does
not have the keyboard: `--window REGEX` matches the focused window's
title or WM class (the launcher has the WM class `torchsnap` and an
empty title), `--shell-modal` requires a GNOME Shell dialog or menu to
hold the keyboard. Text with spaces goes in quotes.

ydotool types the keys of a US keyboard. Typing therefore switches GNOME
to the `us` layout, which provisioning adds, and back to the layout that
was active before, also when typing fails. The switch back waits until
GNOME has handled every key: after the keys the pointer moves by one
pixel and back, and GNOME reports that move only after the keys before
it. While typing, the pointer waits in the bottom left corner, so that
move does not change what the pointer hovers in the app; typing refuses
when an app window covers that corner. Key names for `testbed-key`: `Escape`, `Return`, `Tab`,
`BackSpace`, `space`, `Delete`, `Home`, `End`, `Up`, `Down`, `Left`,
`Right`, `PageUp`, `PageDown`, letters and digits, combined with `ctrl+`,
`shift+`, `alt+`, `super+`.

`testbed-pointer` and `testbed-click` take logical pixels. ydotool's
absolute move does not work on GNOME 48 (the X position stays 0), so the
pointer first moves to the bottom left corner and from there by the
distance, one axis at a time; flat pointer acceleration, set by
provisioning, makes that distance exact. The top left corner is GNOME's
hot corner (`enable-hot-corners` is on), which opens the overview. One
input command runs at a time.

## 8. Clipboard

```sh
just testbed-clipboard-set 'text with spaces'
just testbed-clipboard-get
```

These use `wl-copy` and `wl-paste` in the Pi's GNOME session.
`testbed-clipboard-set` takes the text as one argument and returns once
`wl-copy` has, at most after 5 seconds. Setting the clipboard can close
the launcher: on GNOME 48 it did in 4 of 8 tries, so set the clipboard
before opening the launcher.

## 9. Tray

```sh
just testbed-tray on          # or off, or status
just testbed-tray-icon
just testbed-tray-click       # or right, middle
```

GNOME shows tray icons only with its AppIndicator extension. Debian's
GNOME, like Fedora's, does not turn it on; Ubuntu ships it on.
Provisioning installs it and leaves it off, and `testbed-tray` switches
it, so the app can be tested both ways. `testbed-tray-icon` prints the
position of the app's icon in the top bar, in logical pixels, and
whether its menu is open; it fails when there is no icon.
`testbed-tray-click` clicks the icon's centre. Self-test items 3 and 5
switch the extension on for themselves and back to what they found.

## 10. Screen recording

```sh
just testbed-record-start opening
just testbed-control show
just testbed-record-stop    # copies it to target/linux-testbed/opening.mp4
```

The recording comes from GNOME Shell's screencast method, which stops
as soon as the process that started it disconnects from D-Bus. So
`testbed-record-start` runs `tools/linux-testbed/pi/gnome-record` as the
systemd user unit `torchsnap-testbed-record`, which holds the
connection until `testbed-record-stop` stops it. GNOME Shell writes an
H.264 MP4 at the screen's physical size; a recording of a still 4K
screen had 1.35 frames per second.

## 11. Remote viewing over RDP

The `rdp` step turns on GNOME's remote desktop for the running session:
an RDP client sees and controls the Pi's screen. Connect to the Pi on
port 3389 as user `testbed`; the password is in
`~/.config/torchsnap-testbed/rdp-password` on the Pi, generated at the
first provisioning. The TLS certificate is self-signed and lives in
`~/.local/share/gnome-remote-desktop/` on the Pi.

## 12. Self-test

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
| W | Starts the app if needed and shows the launcher through the control socket: the launcher is among the app's windows with WM class `torchsnap`, a size and keyboard focus. |
| 4 | Opens the launcher through the control socket and checks that it has focus and changes the screen, types a query and checks that the launcher shows it, then presses Escape twice: the first clears the query, the second hides the launcher. |
| L | Opens GNOME Shell's run dialog, types `yz-@[]` and reads the text back from the dialog; it must be exactly that, and the keyboard layout must be the one from before. |
| E | Asks for the launcher state, the app's windows and the clipboard once over plain SSH and once with a runner that clears the whole environment; the answers must be the same. |
| 3 | Needs the tray on, then off: with the AppIndicator extension on, the app's icon is in the top bar; with it off, there is no icon and its area looks different. |
| 5 | Needs the tray on: a click on the app's tray icon opens its menu and changes the screen, Escape closes the menu. |
| 9 | Records the screen while the launcher opens and closes; the recording lasts at least a second, and in the launcher's area its middle frame is clearly lighter than its first (the light search field over the dark screen). |
| 6 | Sets the clipboard through `just testbed-clipboard-set` to text with a space, `$HOME` and a backtick, reads it back, and expects the same text. |
| 2 | Only when named. Sync and build: the build exits 0 and leaves a debug binary written after the build started. |
| R | Only when named. Reboots the Pi, then checks that it is back in GNOME with ydotool's daemon running, `/dev/uinput` usable by the `input` group, RDP listening, the `testbed` keyring unlocking without a prompt, no unsafe-mode notification in GNOME Shell's message tray, and provisioning reporting `0 changed`. |
| P | Only when named; downloads and builds for about 10 minutes. Removes what one installing step installed (the `scdoc` package, `rustup`, `~/.bun`, `just`, `wasm-tools`) and checks that provisioning installs it again and reports `changed` for exactly that step. GNOME's packages cannot be removed without removing GNOME Shell and gdm, so their install runs on the installed system and must leave the installed packages and the display manager choice unchanged. |
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
