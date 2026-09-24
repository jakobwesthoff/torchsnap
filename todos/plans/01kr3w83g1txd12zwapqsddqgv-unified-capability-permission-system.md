---
kind: plan
status: in-progress
---

# Unified Capability Permission System

Cap migration is complete; next phase is provisioning system.

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
`WebsiteMetadataCap`, `IconCacheCap`, `SettingsCap`, `FrecencyCap`,
`PathResolverCap`.

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
├── settings.rs         // SettingsCap (type alias — see 01kr6dyhj4nr82zaz562aw7qgb)
├── frecency.rs         // FrecencyCap (type alias — see 01kr6dyhj4nr82zaz562aw7qgb)
└── path_resolver.rs    // PathResolverCap (wraps GadgetPaths)
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
    pub path_resolver: Option<Arc<PathResolverCap>>,
}
```

## Decided: Manifest `[permissions]` structure

### Design principles

- `[permissions]` is the single source of truth for what caps a gadget
  receives. Configuration sections (`[storage.sql]`, `[settings]`) provide
  additional data but do not imply cap provisioning.
- Manifest keys are kebab-case and match `CapRequest` variant names:
  `filesystem` (not `fs`), `sql-storage`, `website-metadata`,
  `path-resolver`, etc.
- Presence of a key or subsection = cap requested. No explicit
  `requested = true` field needed.
- Two forms: flat `key = true` for parameterless caps, `[permissions.key]`
  subsection for parameterized caps.

### Parameterless caps (flat booleans under `[permissions]`)

```toml
[permissions]
clipboard = true
sql-storage = true
website-metadata = true
icon-cache = true
settings = true
frecency = true
path-resolver = true
```

All default to `false` (not requested) when omitted.

**`settings`** controls whether the gadget receives `SettingsCap` (access
to read/write its own settings via the WIT `settings` interface). The
`[settings]` manifest section still provides default values — but without
`settings = true` under `[permissions]`, the cap is not provisioned and WIT
calls return errors.

**`frecency`** controls whether the gadget can *read* frecency data at
runtime via the WIT `frecency` interface (`is_enabled()`, `top_items()`).
Host-side frecency — recording on execute and score boosting on search
results — is automatic infrastructure and does not require this permission.
Only gadgets that explicitly query frecency data need it (e.g.,
emoji-picker for empty-query most-used display).

**`sql-storage`** gates provisioning of `SqlStorageCap`. The
`CapRequest::SqlStorage` variant carries a `config: SqlStorageConfig` field
(migration file paths). For WASM gadgets, this config is sourced from the
`[storage.sql]` manifest section during the manifest-to-CapRequest
conversion. The cap is only built when `sql-storage = true` is set under
`[permissions]`. If `[storage.sql]` exists without `sql-storage = true`,
that is a manifest validation warning.

### Parameterized caps (subsections under `[permissions]`)

#### `[permissions.opener]`

```toml
[permissions.opener]
schemes = ["https", "http"]
open-path = false      # default
reveal-path = false    # default
```

Maps to `CapRequest::Opener { permissions: OpenerPermissions { ... } }`.

#### `[permissions.http]`

```toml
[permissions.http]
origins = ["https://duckduckgo.com"]
```

Maps to `CapRequest::Http { permissions: HttpPermissions { ... } }`.

#### `[permissions.filesystem]`

```toml
[permissions.filesystem]
read = [
    "${xdg-config}/ZeroTier/One/authtoken.secret",
    "${gadget-data}/cache/*.json",
]
```

Maps to
`CapRequest::Filesystem { permissions: FilesystemPermissions { ... } }`.
`${...}` variable expansion happens at cap construction time (host-side,
using `GadgetPaths` as `PathResolver`), not at manifest parse time.

Renamed from `fs` to `filesystem` to match `CapRequest::Filesystem` and
`FilesystemCap`.

#### `[[permissions.command]]`

```toml
[[permissions.command]]
binary = "/usr/bin/zerotier-cli"
argv = [
    { kind = "enum", values = ["listnetworks", "info", "peers"] },
]
cwd = "${gadget-data}/exec-cwd"
timeout-ms-max = 5000
max-output-bytes = 65536
```

Array-of-tables — each entry is one allowed command rule. Maps to
`CapRequest::Command { permissions: CommandPermissions { ... } }`.

### Changes from current manifest format

| Current | New | Change |
|---------|-----|--------|
| `[permissions.fs]` | `[permissions.filesystem]` | Renamed |
| `website-metadata = true` | `website-metadata = true` | Unchanged |
| `[permissions.opener]` | `[permissions.opener]` | Unchanged |
| `[permissions.http]` | `[permissions.http]` | Unchanged |
| `[[permissions.command]]` | `[[permissions.command]]` | Unchanged |
| _(implicit from `[storage.sql]`)_ | `sql-storage = true` | New, explicit |
| _(always provisioned)_ | `settings = true` | New, explicit opt-in |
| _(always provisioned)_ | `frecency = true` | New, explicit opt-in |
| _(not available)_ | `clipboard = true` | New |
| _(not available)_ | `icon-cache = true` | New |
| _(not available)_ | `path-resolver = true` | New |

### Manifest-to-CapRequest conversion

`WasmGadgetBridge::cap_requests_from_manifest()` converts the parsed
`PermissionsDef` into `Vec<CapRequest>`. The manifest module owns
all `Into` implementations — one per cap type, converting from manifest
deserialization structs to domain types. Boolean caps are collected
directly:

```rust
// Boolean caps
if perms.settings { caps.push(CapRequest::Settings); }
if perms.frecency { caps.push(CapRequest::Frecency); }
// ...

// Parameterized caps — manifest types convert end-to-end via Into.
// Each *PermissionsDef implements From<*Def> for CapRequest in the
// manifest module, composing the intermediate *Permissions conversion.
if let Some(opener) = perms.opener { caps.push(opener.into()); }
if let Some(http) = perms.http { caps.push(http.into()); }
if let Some(fs) = perms.filesystem { caps.push(fs.into()); }
if !perms.command.is_empty() { caps.push(perms.command.into()); }

// Caps with config
if perms.sql_storage {
    if let Some(sql) = &manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
        caps.push(sql.into());
    }
}
```

### Architectural requirements for the permission layer

The permission parsing, conversion, and provisioning pipeline is
load-bearing infrastructure. It must be designed for long-term
maintainability:

- **Clear separation of concerns.** Manifest deserialization
  (`*PermissionsDef` structs) stays in the `wasm/manifest/permissions/`
  module. Domain types (`*Permissions`, `*Config`) and `CapRequest` live
  in `caps/`. `Into` impls bridging the two live in the manifest module
  — each in its own file or clearly grouped, one conversion per cap type,
  no monolithic conversion function.

- **Each cap type is independently traceable.** A developer adding a new
  cap should be able to follow the path from manifest TOML key →
  deserialized struct → `Into` impl → `CapRequest` variant →
  domain `*Permissions`/`*Config` struct → host provisioner → `*Cap`
  construction, touching only files related to that cap type. No shared
  conversion logic that couples unrelated caps.

- **Exhaustive testing.** Every `Into` impl gets unit tests covering the
  mapping from manifest struct to domain type. The host provisioner gets
  integration tests covering the full path from `Vec<CapRequest>` to
  `ProvisionedCaps` — verifying that each requested cap is built and each
  unrequested cap is `None`. Edge cases: empty permissions, unknown fields
  (serde behavior), conflicting declarations.

- **Manifest validation.** Contradictions (e.g., `[storage.sql]` present
  but `sql-storage` not requested) produce clear warnings or errors at
  load time, not silent misbehavior at runtime.

- **Extensibility pattern.** Adding a new cap type requires: (1) new
  domain type (`*Permissions` and/or `*Config`), (2) new `CapRequest`
  variant (unit or struct), (3) new manifest `*Def` struct with `Into`
  impl, (4) new field on `ProvisionedCaps`, (5) provisioner case. All
  mechanical, no existing code modified beyond the match/collection in
  the provisioner.

### Full example: zerotier manifest permissions

```toml
[permissions]
settings = true
sql-storage = true
path-resolver = true

[permissions.http]
origins = ["http://localhost:9993"]

[permissions.filesystem]
read = [
    "${xdg-config}/ZeroTier/One/authtoken.secret",
    "/Library/Application Support/ZeroTier/One/authtoken.secret",
    "/var/lib/zerotier-one/authtoken.secret",
    "C:\\ProgramData\\ZeroTier\\One\\authtoken.secret",
    "${xdg-config}/ZeroTier/saved_networks.json",
]
```

### Full example: bangs manifest permissions

```toml
[permissions]
settings = true
sql-storage = true
website-metadata = true

[permissions.opener]
schemes = ["https", "http"]

[permissions.http]
origins = ["https://duckduckgo.com"]
```

### Full example: emoji-picker manifest permissions

```toml
[permissions]
frecency = true
```

### Full example: calculator manifest permissions

```toml
[permissions]
settings = true
sql-storage = true
```

### Full example: hello-world / template (no caps)

No `[permissions]` section — gadget receives no capabilities.

## Decided: CapRequest system

### Struct variants with explicit `permissions` / `config` fields

`CapRequest` uses enum struct variants to maintain an explicit structural
division between security-relevant permission data and non-security
configuration data. Each parameterized variant uses named fields:
`permissions` for security boundaries, `config` for construction data.
Unit variants remain for caps that need neither.

```rust
enum CapRequest {
    Opener {
        permissions: OpenerPermissions,
    },
    Http {
        permissions: HttpPermissions,
    },
    Filesystem {
        permissions: FilesystemPermissions,
    },
    Command {
        permissions: CommandPermissions,
    },
    SqlStorage {
        config: SqlStorageConfig,
    },
    Clipboard,
    WebsiteMetadata,
    IconCache,
    Settings,
    Frecency,
    PathResolver,  // unit — host builds GadgetPaths internally
}
```

When a future cap needs both permissions and config, its variant simply
carries both fields — no structural change to the enum:

```rust
SomeFutureCap {
    permissions: SomeFuturePermissions,
    config: SomeFutureConfig,
},
```

### Domain types vs. manifest types

`*Permissions` and `*Config` structs are **domain types** that live in
`caps/`. They are the canonical representation used by the host provisioner
and `*Cap` constructors. They carry no serde attributes or
manifest-specific concerns.

Manifest deserialization structs (`*PermissionsDef`, `*ConfigDef`) live in
`wasm/manifest/permissions/` and are serde-annotated for TOML parsing. The
manifest module owns the `Into` implementations that convert from manifest
types to domain types:

```rust
// In wasm/manifest/permissions/opener.rs
impl From<OpenerPermissionsDef> for OpenerPermissions { ... }
```

Dependency direction: `wasm/manifest/` → `caps/`, never the reverse.

Types that both layers need (e.g., `ArgvConstraint` for command rules)
live in `caps/` as the authoritative domain type. The manifest module has
its own serde-decorated mirror type with an `Into` implementation in the
manifest module.

### Cap declaration at registration time (factory pattern)

`requested_caps()` is NOT on the `Gadget` trait. A static trait method
would break object-safety (no `Arc<dyn Gadget>`), and an instance method
can't work because caps must be available before the gadget is
constructed. WASM cap requests come from the manifest (instance-specific
data), which a static method cannot access.

Instead, cap requests are provided at registration time. The host builds
`ProvisionedCaps` from the requests and passes them to a factory closure
that constructs the gadget:

```rust
// Registration API on GadgetHost
fn register<G, F>(
    &mut self,
    gadget_id: &str,
    requests: Vec<CapRequest>,
    factory: F,
    source_kind: GadgetSourceKind,
) where
    G: Gadget + 'static,
    F: FnOnce(Arc<ProvisionedCaps>) -> G,

// Native gadget — cap_requests() is an inherent static method
host.register(
    "app-launcher",
    AppLauncherGadget::cap_requests(),
    AppLauncherGadget::new,
    GadgetSourceKind::BuiltIn,
);

// WASM gadget — cap requests from manifest
host.register(
    manifest.gadget.id.as_str(),
    WasmGadgetBridge::cap_requests_from_manifest(&manifest),
    |caps| WasmGadgetBridge::new(manifest, runtime, caps, ...),
    GadgetSourceKind::Wasm,
);
```

Gadgets receive `Arc<ProvisionedCaps>` as a constructor parameter and
store it as a plain field — no `OnceLock`, no `Mutex<Option<...>>`,
no `unwrap()`.

## Decided: Gadget trait shape (future)

The `Gadget` trait drops `type Caps` and `provision()`. With no associated
type to erase, the trait is object-safe and `AnyGadget` is eliminated.
`Arc<dyn Gadget>` is used directly.

Gadgets receive their `Arc<ProvisionedCaps>` at construction time and own
them as plain fields — no `OnceLock`, no `Mutex<Option<...>>`.
`enable()`/`disable()` are pure lifecycle signals (start/stop background
work) with no capability delivery.

## Decided: ProvisioningContext becomes host-internal

`ProvisioningContext` does not disappear — provisioning still happens, but
moves from `Gadget::provision()` to `GadgetHost`. The host still needs the
raw materials to build caps (`Store` for `SettingsCap`, `FrecencyStore` for
`FrecencyCap`, `IconCache`, `WebsiteMetadataService`, etc.).

What changes: `ProvisioningContext` stops being passed to gadgets. It
becomes host-internal — either fields on `GadgetHost` itself or a
host-internal struct. The critical property is that `AppHandle` never
reaches gadget code. Gadgets see only `Arc<ProvisionedCaps>`.

## Decided: PathResolver dual role

`GadgetPaths` / `PathResolver` serves two completely separate roles:

### Role A — Construction-time (host-internal)

`FilesystemCap::new` and `CommandCap::new` take `&impl PathResolver` to
expand `${gadget-data}`, `${home}`, etc. in manifest patterns at cap
construction time. This happens in the host before any cap is handed to the
gadget. The resolver is consumed and not retained on the cap struct.

This is internal host plumbing, not a capability the gadget requests. The
host always has `GadgetPaths` available when building caps. No manifest
entry is needed for this role.

### Role B — Runtime (gadget-facing)

The `paths::resolve` WIT import lets the gadget call
`resolve("${gadget-data}/foo")` at runtime. Currently served by
`caps.gadget_paths` directly on `WasmGadgetCaps`.

After refactoring, the `paths::resolve` host import reads
`ProvisionedCaps.path_resolver`. `PathResolverCap` wraps
`Arc<dyn PathResolver + Send + Sync>` (not the concrete `GadgetPaths`
type) — it delegates through the trait. If `PathResolverCap` was not
provisioned (gadget didn't request it), the call returns an error. The WIT
interface definition itself stays unchanged — enforcement is host-side.

Gadgets that use `paths::resolve` at runtime (e.g., zerotier) must declare
`PathResolver` in their `CapRequest` / manifest permissions.

## Decided: Setup ordering (done)

All services are created before gadget host construction and registration.
Commit `cecb74a` reordered `lib.rs` setup to establish this. Gadgets are
constructed in a fully initialized environment.

## Decided: Deferred items

- `GadgetSource` remains a WASM bridge internal for now. Future
  encapsulation as a cap tracked in
  `todos/gadget-host/architecture/01kr642tq2r66a9899fw4f0p0p-gadget-source-cap.md`.

## Implementation strategy

### Phase 1: Cap type migration (done)

All caps in `caps/` are implemented. The `*State` wrappers have been
deleted. This migration phase is complete.

### Phase 2: Provisioning restructuring

Tracked in detail in a todo that has since closed (done, removed
2026-09-23).

Summary:
- **Phase A:** Move cap construction from bridge to host
- **Phase B:** Remove `type Caps`, `provision()`, `AnyGadget` from `Gadget`
  trait — trait becomes object-safe, `GadgetSlot` holds `Arc<dyn Gadget>`
- **Phase C:** Introduce `CapRequest` enum (struct variants with
  `permissions`/`config` fields), factory-based registration with
  `cap_requests()` inherent methods and `cap_requests_from_manifest()`,
  domain types in `caps/`, manifest `Into` impls in `wasm/manifest/`
- **Phase D:** Merge `WasmGadgetCaps` into `ProvisionedCaps`

### Phase 3: Future

- Install-time permission UI for plugins
