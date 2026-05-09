# Unified Capability Permission System

## Status: Implementation in progress

## Problem

The current system has two parallel capability/permission architectures:

- **Native gadgets** get raw, unchecked capabilities. They build
  `OpenerCaps::from_app()` directly and call through with no permission
  enforcement.
- **WASM gadgets** have an extra layer: each capability is wrapped in a
  `*State` struct (`OpenerState`, `HttpState`, `FsState`, `CommandState`,
  etc.) that holds both the permission allowlist (from `manifest.toml`) and
  the actual capability. The WASM bridge host implementations check the
  allowlist before delegating.

This creates ad-hoc permission checking spread across `wasm/runtime/host/*.rs`,
`*State` wrapper types that exist only to pair an allowlist with a capability,
and no permission enforcement at all for native gadgets.

## Decided approach: Two-layer model

### Layer 1 — Cap-based access request (binary)

Every gadget — native or WASM — declares upfront which capabilities it needs
via a `Vec<CapRequest>` (enum-based, extensible without touching existing
gadgets). The provisioning system reads this declaration and builds **only**
the requested capabilities. If a gadget didn't request a capability, it
doesn't receive one.

For WASM gadgets, this declaration is extracted from `manifest.toml` (the
existing `[permissions]` section). For native gadgets, it is declared at
registration time (before gadget construction).

The permission declaration is the same representation regardless of gadget
type. There is no distinction between "trusted native" and "sandboxed WASM"
at the permission level.

### Layer 2 — Cap-internal permission checking (fine-grained)

Each capability type defines its own fine-grained permission surface:

- `OpenerCap`: allowed schemes, open-path, reveal-path
- `HttpCap`: allowed origins
- `FilesystemCap`: read/write path patterns (globs)
- `CommandCap`: allowed binary + argv rules
- etc.

These constraints are passed into each capability at construction time and
enforced internally on every call. There is no "unchecked" capability — even
a gadget requesting full access would declare that explicitly (e.g.,
`schemes: ["*"]`), and the capability's internal checking handles wildcards as
a valid permission value.

The WASM bridge's `*State` wrappers and their ad-hoc per-capability checking
become redundant. The bridge host implementations become thin pass-throughs to
the already-permissioned capabilities.

## Key design properties

- **Uniform permission data.** The same `CapRequest` enum is used for WASM
  manifest parsing, native gadget declarations, and (future) install-time UI
  presentation to the user.

- **Capabilities enforce their own permissions.** Only `OpenerCap` knows how
  to check URL schemes. Only `FilesystemCap` knows how path canonicalization
  and glob matching work. The permission logic lives inside the capability,
  not in a wrapper or bridge layer.

- **No unchecked path.** Every capability instance has a permission
  configuration. Wildcards/full-access are valid configurations, not bypasses
  of the system.

## Decided: Capability type system

### Naming convention

All capability types use singular `Cap` suffix: `OpenerCap`, `HttpCap`,
`FilesystemCap`, `CommandCap`, `ClipboardCap`, `SqlStorageCap`,
`WebsiteMetadataCap`, `IconCacheCap`, `SettingsCap`, `FrecencyCap`.

The `Gadget` prefix is dropped (e.g., `FrecencyCap` not `GadgetFrecencyCap`)
— being a cap implies the gadget association.

### Module structure

Each cap lives in its own file under `src-tauri/src/caps/`:

```
src-tauri/src/caps/
├── mod.rs              // ProvisionedCaps, cap_accessor! macro, re-exports
├── opener.rs           // OpenerCap
├── http.rs             // HttpCap
├── filesystem.rs       // FilesystemCap
├── command.rs          // CommandCap
├── clipboard.rs        // ClipboardCap
├── sql_storage.rs      // SqlStorageCap
├── website_metadata.rs // WebsiteMetadataCap
├── icon_cache.rs       // IconCacheCap
├── settings.rs         // SettingsCap
└── frecency.rs         // FrecencyCap
```

### Cap type internals

Each cap type holds:
- The underlying implementation (closures, clients, storage, etc.)
- Permission configuration (schemes, origins, path patterns, etc.)
- Enforces permissions internally on every call

These are built by extracting logic from the current WASM bridge `*State`
wrappers. The `*State` structs are deleted once their corresponding cap
type is complete.

### ProvisionedCaps

All fields are `Option<Arc<XxxCap>>`, uniformly. A `cap_accessor!` macro
generates per-field accessor methods that panic with a clear message if
the cap wasn't provisioned.

```rust
pub struct ProvisionedCaps {
    pub opener: Option<Arc<OpenerCap>>,
    pub http: Option<Arc<HttpCap>>,
    pub filesystem: Option<Arc<FilesystemCap>>,
    pub command: Option<Arc<CommandCap>>,
    pub clipboard: Option<Arc<ClipboardCap>>,
    pub sql_storage: Option<Arc<SqlStorageCap>>,
    pub website_metadata: Option<Arc<WebsiteMetadataCap>>,
    pub icon_cache: Option<Arc<IconCacheCap>>,
    pub settings: Option<Arc<SettingsCap>>,
    pub frecency: Option<Arc<FrecencyCap>>,
}
```

## Decided: Gadget trait shape (future)

The `Gadget` trait drops `type Caps` and `provision()`. With no associated
type to erase, the trait is object-safe and `AnyGadget` is eliminated.
`Arc<dyn Gadget>` is used directly.

Gadgets receive their `Arc<ProvisionedCaps>` at construction time and own
them as plain fields — no `OnceLock`, no `Mutex<Option<...>>`.
`enable()`/`disable()` are pure lifecycle signals (start/stop background
work) with no capability delivery.

## Decided: Setup ordering (done)

All services are created before gadget host construction and registration.
Commit `cecb74a` reordered `lib.rs` setup to establish this. Gadgets are
constructed in a fully initialized environment.

## Decided: Deferred items

- `GadgetSource` and `PathContext` remain WASM bridge internals for now.
  Future encapsulation as caps tracked in
  `todos/architecture/01kr642tq2r66a9899fw4f0p0p-gadget-source-path-context-caps.md`.

## Implementation strategy

### Migration order

One cap at a time. Each migration:

1. Create the cap type in `src-tauri/src/caps/<name>.rs` — extract from
   corresponding `*State` wrapper, move permission checking inside
2. Update WASM bridge to use the new cap type (host impl becomes passthrough)
3. Update native gadgets to use the new cap type
4. Write tests covering default and edge cases
5. Delete the `*State` wrapper
6. Verify everything compiles and passes tests
7. Commit

Start with `OpenerCap` (already partially exists, used by both native
and WASM gadgets), then proceed through the remaining caps.

### Future phases (after all caps migrated)

- Introduce `CapRequest` enum and system-level provisioning
- Remove `type Caps`, `AnyGadget`, `provision()`, `ProvisioningContext`
- Gadgets receive `Arc<ProvisionedCaps>` at construction
- Install-time permission UI for plugins
