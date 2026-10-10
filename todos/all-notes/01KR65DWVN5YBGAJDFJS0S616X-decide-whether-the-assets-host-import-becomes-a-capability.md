---
title: "Decide whether the assets host import becomes a capability"
kind: investigation
component: gadget-host
---
# Decide whether the assets host import becomes a capability

The WASM bridge exposes `assets` and `paths` host imports. The `paths` side is already a cap, so this todo decides only the `assets` side, which is tied to the GadgetSource decision.

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
  a cap (see Relations), this bridge host import would delegate to it.

## TODO

- Evaluate whether `assets` should become a cap or remain bridge-only
  (closely tied to `GadgetSource`, see Relations)
- If yes, define a native `AssetsError` type and create a cap type in `caps/`

## Relations

- Relates to: [Encapsulate GadgetSource as a capability](01KR642TQ2R66A9899FW4F0P0P-encapsulate-gadgetsource-as-a-capability.md), the cap that the assets import would delegate to.
