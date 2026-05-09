# Update Gadget Documentation After Provisioning Refactor

Once the provisioning restructuring is complete, gadget documentation must
be updated to reflect the new system. This todo tracks the scope of
documentation changes needed.

## Manifest `[permissions]` section

- New uniform structure: flat booleans for parameterless caps, subsections
  for parameterized caps.
- Keys renamed to match `CapRequest` variants (kebab-case): `filesystem`
  (not `fs`), `sql-storage`, `website-metadata`, `path-resolver`, etc.
- `settings = true` and `frecency = true` are now opt-in (previously
  implicit/unconditional).
- `path-resolver = true` is new — required for gadgets that call
  `paths::resolve` at runtime.
- `[storage.sql]` still provides configuration (migrations) but no longer
  implies the cap — `sql-storage = true` under `[permissions]` is required.
- Document that host-side frecency (recording + score boosting) is
  automatic and does not require the `frecency` permission. The permission
  only controls the gadget's ability to read frecency data via the WIT
  `frecency` interface at runtime.

## `CapRequest` system

- Document the `CapRequest` enum and all `*Permissions` structs.
- Explain the `From` trait implementations that convert manifest permission
  structs into `CapRequest` variants.
- Document factory-based registration: cap requests are provided at
  registration time, not via a trait method. Native gadgets provide them
  via inherent `cap_requests()` static methods. WASM gadgets derive them
  via `WasmGadgetBridge::cap_requests_from_manifest()`.
- Document why `requested_caps()` is not on the `Gadget` trait: static
  method breaks object-safety, instance method can't work because caps
  must be built before the gadget is constructed.

## Gadget trait changes

- `type Caps` removed — all gadgets receive `Arc<ProvisionedCaps>`.
- `provision()` removed — the host provisions caps externally.
- `AnyGadget` eliminated — `Gadget` trait is now object-safe.
- `enable()` / `disable()` are pure lifecycle signals with no capability
  delivery.
- `Arc<dyn Gadget>` used directly in `GadgetSlot`.

## Provisioning flow

- Document that `GadgetHost` is the single authority that reads permission
  declarations and constructs capabilities.
- `ProvisioningContext` is host-internal — gadgets never see `AppHandle`.
- Gadgets receive `Arc<ProvisionedCaps>` at construction, not at
  `enable()` time.

## PathResolver dual role

- Document the two distinct roles of `GadgetPaths` / `PathResolver`:
  - Role A (construction-time, host-internal): used by `FilesystemCap::new`
    and `CommandCap::new` to expand `${variables}` in manifest patterns.
    Not a requested cap — internal host plumbing.
  - Role B (runtime, gadget-facing): the `paths::resolve` WIT import.
    Requires `path-resolver = true` in manifest permissions.

## WasmGadgetCaps → ProvisionedCaps merge

- `WasmGadgetCaps` no longer exists — all gadgets use `ProvisionedCaps`.
- Document where WASM-specific fields moved:
  - `sql_handle_reps` → `GadgetState` (bridge-internal)
  - `gadget_source` → separate cap (see 01kr642tq2r66a9899fw4f0p0p)
  - `gadget_paths` → replaced by `PathResolverCap` in `ProvisionedCaps`

## Template gadget

- Update the template gadget's `manifest.toml` to demonstrate the new
  `[permissions]` structure with comments explaining each option.

## Gadget developer guide sections affected

- Getting started / manifest reference
- Permission system overview
- Available capabilities and their permission parameters
- Path resolution (construction-time vs runtime)
- Settings and frecency opt-in
