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

## Recipes

- `just testbed-sync`: mirror the working tree (uncommitted changes
  included, ignored files and `.git` left out) to `~/torchsnap` on the
  machine.
