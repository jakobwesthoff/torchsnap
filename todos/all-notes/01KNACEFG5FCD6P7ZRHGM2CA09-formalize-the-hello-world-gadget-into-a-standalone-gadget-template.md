---
title: "Formalize the hello-world gadget into a standalone gadget template"
kind: feature
component: gadgets
---
# Formalize the hello-world gadget into a standalone gadget template

Gadget authors need a reusable, standalone template repository to start from. It covers the full authoring pipeline: a Rust backend, WIT bindings, a frontend Vite config and `.torchsnap` archive packaging. The hello-world gadget in `gadgets/` is the prototype. It should become this template once the architecture is stable.

## Context

The template should reflect patterns proven in a real migration. The calculator gadget's WASM conversion (`gadgets/calculator/`) is already done and serves that role.

The official template targets Rust as the primary language (strategy doc §7.1). Strategy doc references: §8.5 milestone 7, §7.1 (gadget author build pipeline), §7.2 (SDK externalization, deferred), §8.4 (no cargo workspace; gadgets are standalone projects).

## Proposal

The template should include:

- a Rust crate that refers to the WIT by path and builds with plain `cargo build --release`, the `wasm32-wasip2` target set in `.cargo/config.toml` as in `gadgets/`,
- `just` recipes for build, package and dev,
- a Vite config for frontend bundles with SDK externalization (once §7.2 is decided),
- a packaging script that produces the final `.torchsnap` zip.

## Open questions

- SDK externalization (§7.2) is still undecided, and the Vite config depends on it.
