# Evaluate assets and paths as capabilities

## Context

The WASM bridge exposes `assets` and `paths` host imports
(`wasm/runtime/host/assets.rs`, `wasm/runtime/host/paths.rs`) that
currently have no corresponding cap types in `caps/`. They are WASM-only
host imports.

Related: `GadgetSource` and `PathContext` are already tracked for cap
encapsulation in `01kr642tq2r66a9899fw4f0p0p-gadget-source-path-context-caps.md`.
This todo covers the bridge-side host imports that consume those types.

- **Assets**: reads files from the gadget archive via `GadgetSource`.
  Has its own `AssetsError` (variants: `InvalidPath`, `NotFound`,
  `IoError`) with no native Rust equivalent. If `GadgetSource` becomes
  a cap, this bridge code would delegate to it.
- **Paths**: resolves `${...}` permission variables via `PathContext`.
  Has `ResolveError` which already has a native equivalent in
  `crate::wasm::permission_vars::ResolveError`.

## TODO

- Evaluate whether `assets` should become a cap or remain bridge-only
  (closely tied to `GadgetSource` — coordinate with that todo)
- Evaluate whether `paths` should become a cap or remain bridge-only
  (closely tied to `PathContext` — coordinate with that todo)
- If yes, define native error types (especially `AssetsError`) and
  create cap types in `caps/`
