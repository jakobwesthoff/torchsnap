# .devcontainer/ Provenance

The files in this directory — `Dockerfile`, `devcontainer.json`, and
`init-firewall.sh` — are **adapted from**, and in parts directly
reproduce, the Anthropic Claude Code reference devcontainer:

> <https://github.com/anthropics/claude-code/tree/main/.devcontainer>

The upstream repository carries the following license statement:

> © Anthropic PBC. All rights reserved. Use is subject to Anthropic's
> [Commercial Terms of Service](https://www.anthropic.com/legal/commercial-terms).

i.e. the upstream is **not** open source. We reproduce and adapt it here
in reliance on the implied license granted by Anthropic's public
documentation at:

> <https://docs.claude.com/en/docs/claude-code/devcontainer>

which directs users to adopt and customize these files for their own
projects.

## What this means for the rest of Torchsnap

The remainder of this repository is licensed under the **Mozilla Public
License 2.0** (see [`LICENSE`](../LICENSE) at the repo root). The
`.devcontainer/` directory is the **only** exception: the files here
are not covered by that license because they embed third-party content
the project itself cannot relicense.

This means:

- You may use this directory's files to develop Torchsnap in the way
  Anthropic's documentation contemplates (building and running the
  container on your own machine for Claude Code development).
- You should not redistribute these files as MPL-2.0 source or
  repackage them under another license.
- The Torchsnap-specific additions here — the Rust/Bun/Tauri tooling
  additions in the `Dockerfile`, the extended domain allowlist and
  `index.crates.io` verification probe in `init-firewall.sh`, and the
  host-`~/.claude/` bind mounts in `devcontainer.json` — are original
  work by the Torchsnap authors, but they are intermixed with the
  upstream material closely enough that extracting them as a standalone
  MPL-2.0-licensed contribution would be impractical.

## Per-file breakdown

| File                 | Degree of adaptation                                                                      |
|----------------------|-------------------------------------------------------------------------------------------|
| `Dockerfile`         | Substantial adaptation. Base image swapped to `rust:1-bookworm`; Rust toolchain, Bun, Node, wasm-tools, and Tauri Linux libraries added. Upstream structure (apt package grouping, git-delta install, zsh-in-docker bootstrap, narrow-sudoers firewall grant, persistent bash history) retained. |
| `init-firewall.sh`   | Mostly reproduced. Domain allowlist extended with `index.crates.io`, `static.crates.io`, `crates.io`, `static.rust-lang.org`, `sh.rustup.rs`, and an additional post-install verification probe against `index.crates.io`.                                                                   |
| `devcontainer.json`  | Adapted. Torchsnap-specific container name, VS Code extension set (rust-analyzer, wit-idl, even-better-toml, lldb, tailwindcss), named volumes for Cargo / Bun caches, and read-only bind mounts for `~/.claude/CLAUDE.md`, `skills/`, `commands/`, `agents/`, `output-styles/` added.         |

If Anthropic publishes explicit license terms for the reference
devcontainer (e.g. an Apache-2.0 or MIT dedication for that subtree),
this NOTICE should be revisited and the `.devcontainer/` files either
re-released under that license or fully rewritten from scratch under
MPL-2.0.
