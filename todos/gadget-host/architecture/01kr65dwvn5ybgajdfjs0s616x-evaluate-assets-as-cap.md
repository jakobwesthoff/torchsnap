---
kind: decision
status: open
---

# Evaluate assets host import as a capability

## Context

The WASM bridge exposes `assets` and `paths` host imports
(`wasm/runtime/host/assets.rs`, `wasm/runtime/host/paths.rs`).

The `paths` side is resolved: `PathResolverCap` exists in
`src-tauri/src/caps/path_resolver.rs` and is wired into
`ProvisionedCaps`; `paths::resolve` now goes through it rather than
accessing `AppHandle`-derived paths directly.

This todo covers the `assets` side only:

- **Assets**: reads files from the gadget archive via `GadgetSource`.
  Has its own `AssetsError` (variants: `InvalidPath`, `NotFound`,
  `IoError`) with no native Rust equivalent. If `GadgetSource` becomes
  a cap (tracked in `01kr642tq2r66a9899fw4f0p0p`), this bridge host
  import would delegate to it.

## TODO

- Evaluate whether `assets` should become a cap or remain bridge-only
  (closely tied to `GadgetSource` — coordinate with `01kr642tq2r66a9899fw4f0p0p`)
- If yes, define a native `AssetsError` type and create a cap type in `caps/`
