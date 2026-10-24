---
name: linux-testbed
description: Drives the Linux testbed, a Raspberry Pi that the `just testbed-*` recipes reach over SSH, to sync, build, run and test Torchsnap on Linux from the Mac. Use whenever a task needs Torchsnap on Linux, on the Pi or on GNOME, or mentions the testbed, TORCHSNAP_LINUX_HOST or a `testbed-*` recipe.
---

# Linux testbed

The recipes in `just/linux-testbed.just` forward to
`tools/linux-testbed/testbed`. `docs/Howto-test-on-a-raspberry-pi.md`
is the full guide.

## Before the first recipe

- Set `TORCHSNAP_LINUX_HOST` to the SSH target (`user@host`). The
  repository never names the machine: ask the user for the value, and
  do not write it into any file in the repository.
- Recipes stop with `TORCHSNAP_LINUX_HOST is not set` when it is
  missing.
- The recipes use SSH without prompts, so the SSH key for the host must
  be unlocked. If SSH answers "Permission denied (publickey)", ask the
  user to unlock it and do work that needs no machine meanwhile.
- If the machine has a terminal on its screen that runs commands for
  someone to watch (in this project's sessions: `testbed-run`), set
  `TORCHSNAP_LINUX_RUNNER` to its command prefix, for example
  `'~/.local/bin/testbed-run --'`. Every recipe command then runs there.
  Exit status 75 means the runner refused because it was busy; wait and
  retry. Ask the user whether such a terminal exists. With testbed-run,
  follow a long-running unit's log in its own pane so it ends with the
  unit, and close the pane afterwards:
  `testbed-run -w app-log -d -- 'tail -F --pid=$(systemctl --user show -p MainPID --value torchsnap-testbed-app) ~/.cache/torchsnap-testbed/app.log'`,
  then `testbed-run --close app-log`.

## Recipes

- `just testbed-sync`: mirror the working tree (uncommitted changes
  included, ignored files and `.git` left out) to `~/torchsnap` on the
  machine.
- `just testbed-provision`: sync, then run the provision script on the
  machine. Idempotent; a run on a provisioned machine prints
  `provision: 0 changed`. Run it after changing anything under
  `tools/linux-testbed/provision/`.
- `just testbed-reboot`: reboot the machine and wait until its GNOME
  session takes input. Goes over plain SSH, never through the runner.
  Tell the user before rebooting if someone watches the machine's
  screen.
- `just testbed-exec 'COMMAND'`: run one shell command line on the
  machine (through the runner when set).
- `just testbed-build`: sync, build a debug app in the Mac's Linux
  container and copy it to the machine (about 50 s for a one-line Rust
  change; needs Docker and an rsync with zstd, such as Homebrew's, on
  the Mac). `just testbed-build --on-machine`: sync, then
  `just install` and a debug build on the machine (about 2 minutes,
  even for a one-line change).
- `just testbed-app-start` / `just testbed-app-stop`: run or stop the
  build that the last successful `testbed-build` finished; start
  refuses after a failed build. A running app keeps its old binary, so
  restart it after a build. The app runs in the machine's GNOME session
  (unit `torchsnap-testbed-app`, log `~/.cache/torchsnap-testbed/app.log`).
  Start merges `controlChannel.enabled` and `welcome.seenRevision` into
  the app's settings file there.
- `just testbed-app-log [LINES]`: the end of the app's output.
- `just testbed-control METHOD ['JSON']`: one control-socket request
  (`show`, `hide`, `toggle`, `dismiss`, `query '{"text": "…"}'`,
  `status`); exits 1 on an error response.
- `just testbed-screenshot NAME [--window REGEX | --area X,Y,W,H | --monitor N]`:
  a screenshot, copied to `target/linux-testbed/NAME.png`. Look at it
  (Read the PNG) before drawing conclusions; scale it down first, the
  monitor is 4K.
- `just testbed-launcher-state`: `{"visible", "focused", "monitor"}` of
  the launcher. `just testbed-windows`: the app's windows as JSON.
- `just testbed-eval 'JS'`: evaluate JavaScript in GNOME Shell, value
  printed as JSON. For desktop state the other recipes do not cover.
- `just testbed-type --window '^torchsnap$' 'text'`,
  `just testbed-key --window '^torchsnap$' Escape ctrl+u`: keyboard
  input. Always name the target (`--window REGEX` on the focused
  window's title or WM class, or `--shell-modal` for a GNOME Shell
  dialog or menu); exit 4 means the target does not have the keyboard,
  so nothing was typed. In the launcher the first Escape clears the
  query, the second hides it.
- `just testbed-pointer X Y`, `just testbed-click X Y [left|right|middle]`:
  logical pixels.
- `just testbed-tray on|off|status`: GNOME's AppIndicator extension,
  without which GNOME shows no tray icons. Off by default, as on stock
  GNOME; test the app both ways and switch back to what you found.
  `just testbed-tray-icon` (position, menu open) and
  `just testbed-tray-click [button]`.
- `just testbed-headless-start [WxH@SCALE…]` / `just testbed-headless-stop`:
  a second, headless GNOME session with virtual monitors (default
  1280x800@1 and 2560x1600@2). `app-start`, `app-stop`, `app-log`,
  `control`, `eval`, `windows`, `launcher-state` and `screenshot` take
  `--headless`. Keyboard and pointer do not reach it; drive the app
  there through `testbed-control --headless`. No Xwayland there, so the
  clipboard gadget is off.
- `just testbed-record-start NAME` / `just testbed-record-stop`: a
  screen recording, copied to `target/linux-testbed/NAME.mp4`.
- `just testbed-clipboard-set 'text'`, `just testbed-clipboard-get`:
  the session clipboard. Setting it can close the launcher; set it
  before opening the launcher.
- `just testbed-bench-memory 'DESCRIPTION' [--cycles N] [--timing-cycles N] [--env-prefix PREFIX] [--headless]`:
  `tools/bench-memory` on the machine against the current build (PSS
  from `/proc`, GPU memory, GNOME Shell apart); stop the testbed's app
  first. Data files land in `target/linux-testbed/`.
- `just testbed-stop-all`: stop the app, a benchmark's app, recordings and headless
  sessions. Run it when a test is done.
- `just testbed-selftest [--repeat N] [ITEM…]`: the testbed's own checks against the
  machine. Run it after changing the testbed tooling; every item must
  print `PASS`. The items that start the app need a current build
  (`just testbed-build`). Items 2 (build), S (builds from a second
  checkout, three builds), R (reboot), U (undo provision steps)
  and P (reinstall tools, about 10 minutes) run only when named. A
  download step can fail on a DNS hiccup of the machine's Wi-Fi; rerun
  once.

## The machine

- GNOME 48 from Debian with gdm autologin; the testbed's GNOME Shell
  extension opens Shell's Eval, Screenshot and Screencast D-Bus methods
  and closes the overview after login.
- Remote viewing: RDP on port 3389, user `testbed`, password in
  `~/.config/torchsnap-testbed/rdp-password` on the machine. Do not
  print the password into chat or files.
- Provisioning weakens the machine on purpose; the howto's "What
  provisioning opens up" has the list. In short: `Eval` runs any
  JavaScript in GNOME Shell for every process of the login user; the
  default keyring is the unencrypted `testbed` keyring; the RDP password
  is a plain file and reaches `grdctl` on its command line; RDP listens
  on all interfaces with control of keyboard and pointer; the whole
  `input` group can write to `/dev/uinput`; `/etc/gdm3/daemon.conf` is
  replaced as a whole; the dconf default for enabled extensions holds
  only the testbed extension; the rustup, Bun and just install scripts
  come unpinned from the network and run in a shell. Do not provision a
  machine that holds anything worth protecting.
