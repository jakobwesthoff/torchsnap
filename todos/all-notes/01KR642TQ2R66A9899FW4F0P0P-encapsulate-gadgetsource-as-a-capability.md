---
title: "Encapsulate GadgetSource as a capability"
kind: refactor
component: gadget-host
horizon: someday
---
# Encapsulate GadgetSource as a capability

`GadgetSource`, which loads assets from gadget archives, is a WASM-only concept in the bridge internals. Routing it through `ProvisionedCaps` belongs to the unified capability permission system, but the work is deferred because native gadgets have no use for it today.

## Context

`GadgetSource` (asset loading from gadget archives) is a WASM-only concept
living in the WASM bridge internals (`src-tauri/src/wasm/source.rs`).

`PathContext` (platform path resolution for manifest permission variables like
`${home}`, `${xdg-config}`) has already been implemented as `PathResolverCap`
in `src-tauri/src/caps/path_resolver.rs`, wrapping `GadgetPaths`, and is
already wired into `ProvisionedCaps` and the `paths::resolve` host import.

As part of the unified capability permission system (see Relations),
all resources a gadget receives should flow through `ProvisionedCaps`.
`GadgetSource` is deferred because native gadgets have no use for it today.

## TODO

- Define `GadgetSourceCap` type in `src-tauri/src/caps/`
- Integrate it into `ProvisionedCaps`
- Migrate the WASM bridge to consume it from `ProvisionedCaps` instead of
  holding it directly on `WasmGadgetBridge`/`RuntimeState`
- Consider whether native gadgets could benefit from asset loading for bundled
  resources

## Revisit when

Native gadgets need asset loading for bundled resources, the question raised in the last TODO item.
