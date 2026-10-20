---
title: "Publish the gadget template as a standalone repository"
kind: feature
component: gadgets
---
# Publish the gadget template as a standalone repository

Gadget authors outside this repository need a template they can start from without cloning the monorepo. The in-tree starting point exists: `gadgets/template/` is a complete gadget (Rust crate, `manifest.toml`, SQL migrations, a Vite frontend) that the gadget guide tells authors to copy. It cannot be used from outside the monorepo, though. Its crate depends on `../gadget-sdk` by path and builds with the `gadgets/` workspace's `.cargo/config.toml`, and its frontend resolves the SDK from the workspace. What remains is turning it into a template repository that stands alone.

## Context

The template reflects patterns proven in a real migration. The calculator gadget's WASM conversion (`gadgets/calculator/`) is already done and serves that role.

The official template targets Rust as the primary language (strategy doc §7.1). Strategy doc references: §8.5 milestone 7, §7.1 (gadget author build pipeline), §7.2 (SDK package, deferred), §8.4 (no cargo workspace; gadgets are standalone projects).

## Proposal

The standalone template should include:

- a Rust crate that refers to the WIT by path and builds with plain `cargo build --release`, with the `wasm32-wasip2` target set in its own `.cargo/config.toml` as in `gadgets/`,
- `just` recipes for build, package and dev (the monorepo's `build-gadget` and `package-gadget` recipes in `just/gadgets.just` are the model),
- a Vite config for frontend bundles with SDK externalization (once §7.2 is decided),
- a packaging script that produces the final `.torchsnap` zip.

## Open questions

- SDK externalization (§7.2) is still undecided, and the Vite config depends on it.
- Where the WIT and the Rust SDK crate come from for an outside author (a published crate, a git dependency or a vendored copy), as the path dependency on `../gadget-sdk` does not work outside the monorepo.
- Whether `gadgets/template/` stays as the source of truth and the standalone repository is generated from it, or the other way round.
