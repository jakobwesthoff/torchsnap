# Gadget Template Project

Create a reusable, standalone gadget template repository that gadget authors can use as a starting point. Covers the full authoring pipeline: cargo-component Rust backend, WIT bindings, frontend Vite config, and `.torchsnap` archive packaging.

**Strategy doc:** §8.5 milestone 7, §7.1 (gadget author build pipeline), §7.2 (SDK externalization — deferred), §8.4 (no cargo workspace; gadgets are standalone projects)

**Status:** not started

**Depends on:** the calculator gadget's WASM conversion, already done (see `gadgets/calculator/`); template should reflect patterns proven in a real migration

**Notes:** The official template targets Rust as the primary language (§7.1). Should include: `cargo-component` project structure referencing WIT by path, `just` recipes for build/package/dev, a Vite config for frontend bundles with SDK externalization (once §7.2 is decided), and a packaging script that produces the final `.torchsnap` zip. The hello-world gadget in `gadgets/` is the prototype; formalize it into a template suitable for external gadget authors once the architecture is stable.
