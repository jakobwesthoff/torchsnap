# Encapsulate GadgetSource as a capability

## Context

`GadgetSource` (asset loading from gadget archives) is a WASM-only concept
living in the WASM bridge internals (`src-tauri/src/wasm/source.rs`).

`PathContext` (platform path resolution for manifest permission variables like
`${home}`, `${xdg-config}`) has already been implemented as `PathResolverCap`
in `src-tauri/src/caps/path_resolver.rs`, wrapping `GadgetPaths`. The wiring
of `PathResolverCap` into `WasmGadgetCaps` and the `paths::resolve` host import
is tracked as part of the provisioning restructuring
(`01kr6d8ysxjemp6xfqees67qax`).

As part of the unified capability permission system (see
`todos/plans/01kr3w83g1txd12zwapqsddqgv-unified-capability-permission-system.md`),
all resources a gadget receives should flow through `ProvisionedCaps`.
`GadgetSource` is deferred because native gadgets have no use for it today.

## TODO

- Define `GadgetSourceCap` type in `src-tauri/src/caps/`
- Integrate it into `ProvisionedCaps`
- Migrate the WASM bridge to consume it from `ProvisionedCaps` instead of
  holding it directly on `WasmGadgetCaps`
- Consider whether native gadgets could benefit from asset loading for bundled
  resources
