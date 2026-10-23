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
  retry. Ask the user whether such a terminal exists.

## Recipes

- `just testbed-sync`: mirror the working tree (uncommitted changes
  included, ignored files and `.git` left out) to `~/torchsnap` on the
  machine.
- `just testbed-provision`: sync, then run the provision script on the
  machine. Idempotent; a run on a provisioned machine prints
  `provision: 0 changed`. Run it after changing anything under
  `tools/linux-testbed/provision/`.
- `just testbed-selftest [ITEM…]`: the testbed's own checks against the
  machine. Run it after changing the testbed tooling; every item must
  print `PASS`.
