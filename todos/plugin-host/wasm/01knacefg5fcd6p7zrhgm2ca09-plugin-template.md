# Plugin Template Project

Create a reusable, standalone plugin template repository that plugin authors can use as a starting point. Covers the full authoring pipeline: cargo-component Rust backend, WIT bindings, frontend Vite config, and `.torchsnap` archive packaging.

**Strategy doc:** §8.5 milestone 7, §7.1 (plugin author build pipeline), §7.2 (SDK externalization — deferred), §8.4 (no cargo workspace; plugins are standalone projects)

**Status:** not started

**Depends on:** `01knacefg5fcd6p7zrhgm2ca07` (calculator-conversion — template should reflect patterns proven in a real migration)

**Notes:** The official template targets Rust as the primary language (§7.1). Should include: `cargo-component` project structure referencing WIT by path, `just` recipes for build/package/dev, a Vite config for frontend bundles with SDK externalization (once §7.2 is decided), and a packaging script that produces the final `.torchsnap` zip. The hello-world plugin in `plugins/` is the prototype; formalize it into a template suitable for external plugin authors once the architecture is stable.
