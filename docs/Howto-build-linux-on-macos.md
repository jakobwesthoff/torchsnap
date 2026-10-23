# How to build Torchsnap for Linux on a Mac

The recipes in `just/linux-build.just` compile Torchsnap for Linux
aarch64 on an Apple silicon Mac, inside a Docker container that runs
Debian 13 (trixie), the base of Raspberry Pi OS. A Raspberry Pi
compiles Torchsnap slowly, and Linux release builds will probably need
such an environment later.

Today the container runs `cargo check` and clippy for Linux, so code
behind `cfg(target_os = "linux")` written on the Mac gets compiled and
linted before it reaches a Linux machine.

## Requirements

- An Apple silicon Mac with Docker whose server runs `linux/arm64`, so
  the container runs natively without emulation. Check with
  `docker version --format '{{.Server.Arch}}'`, which prints `arm64`.
  The recipes were built and measured with OrbStack.
- `jq` and `shellcheck` for `just test-tools` and for checking the
  scripts.

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
| lint work volume | `torchsnap-linux-build-lint-<hash>`, one per checkout | label, plus a label naming the checkout |

Each removal checks the exact name as well. Nothing runs a global
prune, and the build cache is pruned only after a listing of its marked
records worked. When a rebuild changes the image, exactly the image it
replaced is removed, once no container of these recipes runs. An image
someone builds `FROM torchsnap-linux-build` inherits the label; while
untagged, it shows up in `just linux-build-status`, and
`just linux-build-clean --all` removes it. An object that a running
container uses is kept and reported as `in use, kept`.

Measured on 2026-10-23 after linting this repository twice:

| Object | Size |
|---|---|
| image | 2.26 GB |
| build cache | 2.12 GB |
| cargo registry | 1.28 GB |
| bun cache | 0.30 GB |
| one lint work volume | 3.39 GB |

Before every lint, the recipes free space in this order, and print what
each step removed and how much it freed:

1. A lint work volume whose checkout no longer exists goes.
2. At most `TORCHSNAP_LINUX_BUILD_MAX_LINT_VOLUMES` lint work volumes
   (default 3) stay, counting the one the lint uses; the least recently
   used go first.
3. While the image, the build cache and all volumes together take more
   than `TORCHSNAP_LINUX_BUILD_BUDGET_GB` (default 40), other lint work
   volumes go, least recently used first, then the build cache. The
   image and the cargo and bun caches are never removed automatically,
   and neither is the volume the lint is about to use. If that is not
   enough, a warning says so and the lint goes on.

With the sizes above, the image, the build cache, the two download
caches and three lint work volumes take 16.1 GB of the default 40 GB.
Both variables take a whole number from 1 to 99999.

`status` reads the last use of each work volume with a short container
from the image, so without the image it shows sizes only and the limits
skip their work. A removed volume is created again, empty, on its next
use; that run then compiles from scratch.

`target/linux-build/` of the checkout that runs the recipe is counted
too, those of other checkouts are not.
