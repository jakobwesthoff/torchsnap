# Re-evaluate the Claude devcontainer: keep, remove or replace

**Kind:** decision
**Status:** open, raised 2026-09-21

## What it is

A Docker environment for running Claude Code on the repository in
isolation, added 2026-04-20 in `31a1106` ("Add devcontainer setup to run
more isolated claude"). It is adapted from Anthropic's reference Claude
Code devcontainer.

- `.devcontainer/Dockerfile` (286 lines): image based on
  `rust:<RUST_VERSION>-bookworm`, adds the Tauri build dependencies, bun,
  just, Claude Code, git-delta and a zsh setup (zsh-in-docker).
- `.devcontainer/devcontainer.json` (115 lines): build args, mounts (the
  repository, `~/.claude` config read-only, named volumes for the
  cargo/bun caches and the Claude login), VS Code settings.
- `.devcontainer/init-firewall.sh` (226 lines): restricts the container's
  outbound network.
- `.devcontainer/NOTICE.md`: provenance. The upstream files are
  "© Anthropic PBC. All rights reserved", used under the implied license
  from Anthropic's devcontainer documentation. These files are not
  covered by the project's MPL-2.0 license.
- `just/devcontainer.just`: `just devcontainer` (build, start, enter via
  `bunx @devcontainers/cli`) and `just devcontainer-reset-caches`.

It plays no part in building, testing or shipping Torchsnap, and nothing
in CI uses it.

## Why re-evaluate

- **Maintenance.** It carries its own pins (git-delta, zsh-in-docker,
  the Rust base image, which must match `rust-toolchain.toml`) in two
  places each, Dockerfile `ARG` defaults and `devcontainer.json` build
  args. The 2026-09-21 dependency update had to bump them by hand
  (`8e6c588`), and the git-delta bump needed care: 0.19.0/0.19.1 `.deb`s
  require glibc 2.39, which bookworm lacks.
- **Licensing.** It is the only part of the repository under someone
  else's all-rights-reserved terms, held by an implied license.
- **Unclear use.** It was last changed on 2026-05-06 (a path rename). The
  user still uses it (2026-09-21), but whether the isolation it gives is
  still needed, or better provided another way, has not been looked at.

## Options

1. **Keep and maintain.** Update pins alongside dependency updates;
   keep `RUST_VERSION` equal to the `rust-toolchain.toml` channel.
2. **Remove.** Delete `.devcontainer/` and `just/devcontainer.just`, the
   `import` line in `Justfile`, and the `RUST_VERSION` note in
   `rust-toolchain.toml`. The named Docker volumes (`torchsnap-*`) stay on
   machines that ran it until removed by hand.
3. **Replace.** Check what other isolation mechanisms exist for running
   Claude Code by then (for example sandboxing features in Claude Code
   itself, or a generic devcontainer kept outside this repository) and
   whether one covers the need with less repository-specific upkeep.

## Related

`todos/2026-07-02-review/build/01kwfz4kkaq7spwnm2ncket1fq-missing-mpl-header-init-firewall.md`
proposes adding the MPL-2.0 header to `.devcontainer/init-firewall.sh`.
That conflicts with `.devcontainer/NOTICE.md`, which states these files
are not MPL-licensed. Resolve it as part of this decision: removing the
devcontainer makes it moot; keeping it probably means closing that todo
with a note instead of adding the header.
