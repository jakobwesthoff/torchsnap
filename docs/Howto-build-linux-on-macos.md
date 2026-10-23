# How to build Torchsnap for Linux on a Mac

The recipes in `just/linux-build.just` compile Torchsnap for Linux
aarch64 on an Apple silicon Mac, inside a Docker container that runs
Debian 13 (trixie), the base of Raspberry Pi OS. A Raspberry Pi
compiles Torchsnap slowly, and Linux release builds will probably need
such an environment later.

The container builds Torchsnap without packaging, in the debug or
release profile, and runs `cargo check` and clippy for Linux, so code
behind `cfg(target_os = "linux")` written on the Mac gets compiled and
linted before it reaches a Linux machine.

## Requirements

- An Apple silicon Mac with Docker whose server runs `linux/arm64`, so
  the container runs natively without emulation. Check with
  `docker version --format '{{.Server.Arch}}'`, which prints `arm64`.
  The recipes were built and measured with OrbStack.
- `jq` and `shellcheck` for `just test-tools` and for checking the
  scripts.

## Build

```sh
just linux-build             # debug
just linux-build --release   # release
```

The recipe builds the image if needed, copies this checkout into a
Docker volume (the same way as lint, below), runs `just install` and
`just build --config '{"bundle":{"active":false}}'` there, and lays out
the result in `target/linux-build/<profile>/`:

```text
target/linux-build/debug/
  bin/torchsnap
  lib/Torchsnap/gadgets/<id>.torchsnap   one per entry of gadgets/bundled.toml
  lib/Torchsnap/gadget-document.icns     the other bundle resources, unused on Linux
  lib/Torchsnap/Assets.car
  build-packages.txt                     the image's Debian packages and versions
```

Copy the whole directory. A Linux binary outside a cargo target
directory looks for its resources in `../lib/Torchsnap/` next to its
own directory, the layout of Tauri's Debian package, so `bin/` and
`lib/` must stay together; the frontend is compiled into the binary.
`build-packages.txt` records what the binary was built against, since
the image takes its packages from the live trixie archive.

The binary knows the commit it was built from: the recipe mounts the
checkout's git directory read-only into the container. Outside a git
checkout the commit shows as `unknown`.

A debug binary also loads the gadgets under `<work path>/gadgets`, the
source tree's path inside the container, which only exists there. Set
`TORCHSNAP_LINUX_BUILD_WORKDIR` (default `/work`) to the absolute path
where the same tree sits on the target machine, and the debug binary
finds the unlisted gadgets there as well. Changing the path makes the
next build compile from scratch.

Only one build runs at a time: a second one fails at `docker run`
because the container name `torchsnap-linux-build` is taken.

### How long it takes

Measured on 2026-10-23, twice each, on an Apple M4 Pro (14 cores,
24 GB, OrbStack giving its Linux machine 14 CPUs and 12.6 GB), next to
`just install` plus `just build` without bundling natively on a
Raspberry Pi 5 with Raspberry Pi OS 13:

| Build | Mac container, debug | Mac container, release | Raspberry Pi 5, debug |
|---|---|---|---|
| clean (download caches warm) | 123 s, 119 s | 140 s, 143 s | 802 s plus 96 s `just install` |
| nothing changed | 43 s, 41 s | 34 s, 34 s | 107 s, 113 s |
| one line in `src-tauri/src/lib.rs` | 36 s, 36 s | 34 s, 33 s | 108 s, 110 s |
| one line in the launcher's frontend | 41 s, 41 s | 33 s, 33 s | 106 s, 110 s |

Every `just build` compiles the host crate again, also when nothing
changed: the frontend build writes `dist/` anew, and the host's build
script watches it.

Copying `target/linux-build/debug/` to the Raspberry Pi over its Wi-Fi
with rsync, measured in the same runs:

| Copy | plain | `--compress --compress-choice=zstd` |
|---|---|---|
| into an empty directory (793 MB) | 187 s, 188 s | 43 s, 43 s (185 MB sent) |
| after the one-line `lib.rs` change | 2.9 s, 2.7 s (147 kB sent) | 2.4 s, 2.4 s (12 kB sent) |

rsync's delta transfer sends only the changed parts of the 788 MB debug
binary. The copies used Homebrew's rsync 3.5.1 on the Mac and rsync
3.5.0 on the Pi, which both list zstd. macOS's own `/usr/bin/rsync` is
openrsync and has no `--compress-choice`.

## Check a build on a Linux machine

```sh
export TORCHSNAP_LINUX_HOST=user@pi.local
just linux-build-check              # or --release
just linux-build-run-check          # or --release
```

Both recipes reach the machine over ssh and copy the build with rsync
to `~/linux-build-check/<profile>/` there; rsync sends only what
changed. If `TORCHSNAP_LINUX_RUNNER` is set, every command on the
machine runs through it as a prefix (for example a script that runs the
command in a terminal someone watches); the copy itself is plain rsync.

`just linux-build-check` passes when both hold:

- glibc: the highest glibc symbol version the binary needs (read with
  the image's `objdump`) is not newer than the machine's
  (`getconf GNU_LIBC_VERSION`);
- ldd: `ldd` on the machine reports no library and no symbol version as
  `not found`.

On a Raspberry Pi 5 with Raspberry Pi OS 13 the debug build needs glibc
2.39, the Pi has 2.41, and ldd finds every library. The binary links
WebKitGTK 4.1, GTK 3, libsoup 3 and JavaScriptCore; it loads
`libayatana-appindicator3.so.1` for the tray icon only when it runs,
so ldd does not list it and the machine needs the package that has it
(`libayatana-appindicator3-1` on Debian).

`just linux-build-run-check` starts the copied build and asks it for
its status over its control socket. It passes when the build answers
and its environment holds nothing of a desktop session the machine
runs. The machine needs `labwc`, `dbus-run-session`, `socat` and a
systemd user session. Everything runs in the transient user unit
`linux-build-check`:

- a headless labwc is the display server, so no window shows on the
  machine's screen, and labwc starts its own Xwayland for X11 clients;
  `labwc -C` with an empty directory keeps labwc from running the
  system's autostart;
- `dbus-run-session` gives the build its own session bus, so a
  Torchsnap already running in the desktop session does not count as
  the same instance;
- its data, config and cache directories are
  `~/linux-build-check/<profile>-home/`, with the control socket
  switched on and the welcome window marked as seen;
- the unit starts from an empty environment plus a short list of
  variables, since a systemd user unit otherwise inherits the desktop
  session's variables.

The unit is stopped when the check ends, also when it fails; the
build's output goes to `~/linux-build-check/<profile>-run.log`. Only
files under `~/linux-build-check/` stay on the machine.

## Lint a checkout for Linux

```sh
just linux-build-lint                 # this checkout
just linux-build-lint ../other-tree   # any other checkout of Torchsnap
```

The recipe builds the image if needed, copies the checkout into a
Docker volume and runs `just install`, `just check-crates` and
`just lint-crates` there: `cargo check` and
`cargo clippy --all-targets -- -D warnings` on `src-tauri/`. It exits
with the first failing step's status.

- The checkout is mounted read-only. The container copies it with
  rsync into its own volume, leaving out what `.gitignore` files name
  and `.git/`, `.claude/` and `target/` at the top. Untracked files
  that no `.gitignore` names are copied too.
- `just install` runs every time, because tauri-build needs generated,
  gitignored files such as `src-tauri/icons/Assets.car`, which the copy
  leaves out. It downloads the bang database each time, so lint needs
  the network.
- The scripts come from the checkout that runs the recipe, so a checkout
  without `tools/linux-build/` can be linted.
- A checkout whose `rust-toolchain.toml` pins another Rust release than
  this one is refused, since the image carries only this checkout's
  release.
- Each checkout gets its own work volume, so lints of different
  checkouts keep their compiled state apart.
- Ctrl-C stops the run.

## What the image contains

`tools/linux-build/Dockerfile` builds the image `torchsnap-linux-build`:

- Base: `debian:trixie-20261020` pinned by digest. Packages come from
  the live trixie archive, the one Raspberry Pi OS updates from.
- Tauri's Debian build dependencies (WebKitGTK 4.1, GTK 3, libsoup 3,
  ayatana appindicator, librsvg, xdo, OpenSSL, xcb) and what the
  recipes call: zip, ImageMagick 7, jq, curl, tzdata, git, rsync.
- rustup 1.29.2 with the Rust release `rust-toolchain.toml` pins, the
  `wasm32-wasip2` target, clippy and rustfmt.
- bun 1.4.4, just 1.58.0, wasm-tools 1.246.2.

Every download is checked against a SHA-256 in the Dockerfile, and the
last step fails the image build if a tool reports another version. To
bump Rust, change `rust-toolchain.toml`; the next run rebuilds the
image. To bump another tool, change its version and SHA-256 in the
Dockerfile.

## Disk use

```sh
just linux-build-status        # what everything takes, and the total
just linux-build-clean         # drop the work volumes and target/linux-build/
just linux-build-clean --all   # drop everything listed by status
```

The Docker on the Mac is usually shared with other projects, so the
recipes only ever remove objects they created and marked:

| Object | Name | Marked by |
|---|---|---|
| image | `torchsnap-linux-build:latest` | label `org.torchsnap.linux-build=1` |
| build cache | records of that image's build | every `RUN` runs through `/usr/bin/env TORCHSNAP_LINUX_BUILD=1 /bin/sh -c`, which BuildKit copies into the record's description |
| cargo registry | volume `torchsnap-linux-build-cargo` | label |
| bun cache | volume `torchsnap-linux-build-bun` | label |
| build work volume | `torchsnap-linux-build-work` | label |
| lint work volume | `torchsnap-linux-build-lint-<hash>`, one per checkout | label, plus a label naming the checkout |

Each removal checks the exact name as well. Nothing runs a global
prune, and the build cache is pruned only after a listing of its marked
records worked. When a rebuild changes the image, exactly the image it
replaced is removed, once no container of these recipes runs. An image
someone builds `FROM torchsnap-linux-build` inherits the label; while
untagged, it shows up in `just linux-build-status`, and
`just linux-build-clean --all` removes it. An object that a running
container uses is kept and reported as `in use, kept`.

Measured on 2026-10-23 after linting this repository twice and building
it in both profiles:

| Object | Size |
|---|---|
| image | 2.26 GB |
| build cache | 2.12 GB |
| cargo registry | 1.28 GB |
| bun cache | 0.30 GB |
| build work volume, debug and release built | 14.17 GB |
| one lint work volume | 3.39 GB |
| `target/linux-build/debug/` | 792.9 MB |
| `target/linux-build/release/` | 84.0 MB |

Before every build and lint, the recipes free space in this order, and
print what each step removed and how much it freed:

1. A lint work volume whose checkout no longer exists goes.
2. At most `TORCHSNAP_LINUX_BUILD_MAX_LINT_VOLUMES` lint work volumes
   (default 3) stay, counting the one a lint uses; the least recently
   used go first.
3. While the image, the build cache and all volumes together take more
   than `TORCHSNAP_LINUX_BUILD_BUDGET_GB` (default 40), lint work
   volumes go, least recently used first, then the build cache, then
   the build work volume. A run never removes the volume it is about to
   use, and a lint leaves the build work volume alone while a build
   runs. The image and the cargo and bun caches are never removed
   automatically. If that is not enough, a warning says so and the run
   goes on.

With the sizes above, the image, the build cache, the two download
caches, the build work volume and three lint work volumes take 30.3 GB
of the default 40 GB. Both variables take a whole number from 1 to
99999.

`status` reads the last use of each work volume with a short container
from the image, so without the image it shows sizes only and the limits
skip their work. A removed volume is created again, empty, on its next
use; that run then compiles from scratch.

`target/linux-build/` of the checkout that runs the recipe is counted
too, those of other checkouts are not.

## Packaging (outline, not done)

The recipes build without packaging. A `.deb`, AppImage or rpm would
take at least these steps, none of which has been tried:

- run `tauri build` with `bundle.active` and the Linux bundle targets
  in the same container, and add whatever Tauri's bundler needs per
  format to the image;
- keep the resource layout: a binary in `/usr/bin` finds its resources
  in `/usr/lib/Torchsnap/`, which is the layout `target/linux-build/`
  copies;
- declare the runtime packages in the package metadata, the libraries
  the binary links (WebKitGTK 4.1, GTK 3, libsoup 3, JavaScriptCore)
  plus the appindicator library it loads at runtime;
- add a `.desktop` file and Linux icons, and decide on signing;
- build for x86_64, which needs another image or emulation on an Apple
  silicon Mac.
