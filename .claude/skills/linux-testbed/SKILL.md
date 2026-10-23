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
- `just testbed-build`: sync, then `just install` and a debug build on
  the machine (about 2 minutes, even for a one-line change).
- `just testbed-app-start` / `just testbed-app-stop`: run or stop the
  debug app in the machine's GNOME session (unit
  `torchsnap-testbed-app`, log `~/.cache/torchsnap-testbed/app.log`).
  Start merges `controlChannel.enabled` and `welcome.seenRevision` into
  the app's settings file there.
- `just testbed-app-log [LINES]`: the end of the app's output.
- `just testbed-control METHOD ['JSON']`: one control-socket request
  (`show`, `hide`, `toggle`, `dismiss`, `query '{"text": "…"}'`,
  `status`); exits 1 on an error response.
- `just testbed-selftest [ITEM…]`: the testbed's own checks against the
  machine. Run it after changing the testbed tooling; every item must
  print `PASS`. Items R (reboot) and U (undo provision steps) run only
  when named.

## The machine

- GNOME 48 from Debian with gdm autologin; the testbed's GNOME Shell
  extension opens Shell's Eval, Screenshot and Screencast D-Bus methods
  and closes the overview after login.
- Remote viewing: RDP on port 3389, user `testbed`, password in
  `~/.config/torchsnap-testbed/rdp-password` on the machine. Do not
  print the password into chat or files.
