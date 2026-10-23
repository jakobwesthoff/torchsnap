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
```

syncs the working tree and runs `tools/linux-testbed/provision/provision`
on the Pi as the login user, which needs `sudo` without a password
there. Every step checks first and changes only what is missing, so a
second run reports `provision: 0 changed, …`. Each step prints `ok`,
`changed` or `failed` with its name; a failed step stops the run with
exit status 1.

The steps:

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

The provision script has not been run on a freshly installed system: it
was checked on a Pi first set up by hand with the same commands, by
running it there.

A command run over SSH does not get the `PATH` entries these installers
add to the shell profile, so the provision script puts `~/.cargo/bin`,
`~/.bun/bin` and `~/.local/bin` on `PATH` itself.

## 4. Self-test

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
