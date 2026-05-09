# Encapsulate GadgetSource and PathContext as capabilities

## Context

`GadgetSource` (asset loading from gadget archives) and `PathContext`
(platform path resolution for manifest permission variables like `${home}`,
`${xdg-config}`) are currently WASM-only concepts living in the WASM bridge
internals.

As part of the unified capability permission system (see
`todos/plans/01kr3w83g1txd12zwapqsddqgv-unified-capability-permission-system.md`),
all resources a gadget receives should flow through `ProvisionedCaps`. These
two are deferred because native gadgets have no use for them today, and
`PathContext` is specifically tied to manifest variable resolution.

## TODO

- Define `GadgetSourceCap` and `PathContextCap` types in `src-tauri/src/caps/`
- Integrate them into `ProvisionedCaps`
- Migrate the WASM bridge to consume them from `ProvisionedCaps` instead of
  holding them directly on `WasmGadgetCaps`
- Consider whether native gadgets could benefit from either (e.g., asset
  loading for bundled resources)
