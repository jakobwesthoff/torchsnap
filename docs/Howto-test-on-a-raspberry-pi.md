# How to test Torchsnap on a Raspberry Pi

The Linux testbed is a Raspberry Pi that the `just testbed-*` recipes
drive from the Mac over SSH. This guide is the counterpart to
[How to build Torchsnap on Fedora 43](Howto-build-on-fedora-43.md): that
one sets up a Linux machine to develop on, this one sets up a machine
that recipes on the Mac work with.

## 1. Requirements

- SSH login to the Pi with a key, without a password prompt.
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
