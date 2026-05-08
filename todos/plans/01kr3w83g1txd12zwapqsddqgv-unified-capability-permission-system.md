# Unified Capability Permission System

## Status: Design discussion in progress

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

- `OpenerCaps`: allowed schemes, open-path, reveal-path
- `HttpCaps`: allowed origins
- `FsCaps`: read/write path patterns (globs)
- `CommandCaps`: allowed binary + argv rules
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

- **Capabilities enforce their own permissions.** Only `OpenerCaps` knows how
  to check URL schemes. Only `FsCaps` knows how path canonicalization and glob
  matching work. The permission logic lives inside the capability, not in a
  wrapper or bridge layer.

- **No unchecked path.** Every capability instance has a permission
  configuration. Wildcards/full-access are valid configurations, not bypasses
  of the system.

## Decided: Gadget trait shape

The `Gadget` trait drops `type Caps` and `provision()`. With no associated
type to erase, the trait is object-safe and `AnyGadget` is eliminated.
`Arc<dyn Gadget>` is used directly.

Gadgets receive their `Arc<ProvisionedCaps>` at construction time and own
them as plain fields — no `OnceLock`, no `Mutex<Option<...>>`.
`enable()`/`disable()` are pure lifecycle signals (start/stop background
work) with no capability delivery.

```rust
trait Gadget: Send + Sync {
    fn id(&self) -> &str;
    fn enable(&self);
    fn disable(&self);
    fn execute(&self, entry: &ScoredEntry, action_id: &ActionId) -> anyhow::Result<PostAction>;
    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<GadgetResponse>;
    // ...other methods without caps parameters
}
```

`ProvisionedCaps` is an Option-field struct:

```rust
struct ProvisionedCaps {
    pub opener: Option<Arc<OpenerCaps>>,
    pub http: Option<Arc<HttpCaps>>,
    pub fs: Option<Arc<FsCaps>>,
    pub command: Option<Arc<CommandCaps>>,
    pub clipboard: Option<Arc<ClipboardCaps>>,
    pub sql: Option<Arc<SqlCaps>>,
    pub website_metadata: Option<Arc<WebsiteMetadataCaps>>,
    pub icon_cache: Option<Arc<IconCache>>,
}
```

## Decided: Registration and construction split

Gadget registration and construction are separated into two phases so that
construction happens after all host services are ready.

**Registration** (early in setup): declares the gadget's identity, source
kind, permission requirements, and a factory to construct it. The gadget
does not exist yet.

**Construction** (after services are ready): the system reads the
registration's permission declaration, builds `ProvisionedCaps`, calls the
factory to construct the gadget with its caps already available.

This eliminates `ProvisioningContext` — gadgets never see raw `AppHandle`
or other platform handles. The system is the sole builder of capabilities.

## Implementation phasing

### Phase 1: Defer gadget construction (prerequisite, no capability changes)

Move gadget instantiation from early in `setup()` to after all services
(`AppHandle`, `Store`, `IconCache`, `WebsiteMetadataService`, etc.) are
ready. The current `provision()` logic collapses into the constructors.
`AnyGadget` and `type Caps` can remain during this phase — the goal is
purely to reorder setup so that gadgets are born in a fully initialized
environment.

This is a mechanical refactor with no design decisions about the capability
system.

### Phase 2: Unified capability/permission system

Introduce the `CapRequest` enum, `ProvisionedCaps`, permission-constrained
capability construction, and the registration/factory pattern. Remove
`type Caps`, `AnyGadget`, `provision()`, `ProvisioningContext`, and the
WASM bridge's `*State` wrappers. Each capability type absorbs its own
permission checking logic.

## Future: Install-time permission UI

Parsing permission declarations from manifests/registrations to display to
the user before installing a plugin. The uniform `CapRequest` data enables
this directly.
