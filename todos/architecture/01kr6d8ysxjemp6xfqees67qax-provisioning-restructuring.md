# Provisioning Restructuring: Host-Owned Cap Construction

## Problem

The cap type migration (replacing `*State` wrappers with `*Cap` types) is
complete, but the *construction* of caps hasn't moved. The WASM bridge still
builds all caps itself inside `WasmGadgetBridge::provision()`, assembling
`WasmGadgetCaps` from raw `ProvisioningContext`. Native gadgets build their
own caps in `provision()` from the same context. This creates several issues:

1. **Bridge constructs caps it should only receive.** The bridge holds raw
   manifest permission data (`opener_schemes`, `http_origins`,
   `fs_patterns_raw`, `command_rules_raw`, etc.) and constructs caps from
   them. The host should build caps based on declared permissions and hand
   them to the gadget pre-built.

2. **`ProvisioningContext` exposes raw `AppHandle`.** Gadgets can build
   unrestricted capabilities from `AppHandle`, bypassing any permission
   system. The context should provide pre-constrained caps, not raw
   platform handles.

3. **`WasmGadgetCaps` duplicates `ProvisionedCaps`.** Both structs hold
   the same `Option<Arc<*Cap>>` fields. `WasmGadgetCaps` also holds
   WASM-specific fields (`gadget_source`, `gadget_paths`,
   `sql_handle_reps`) that don't belong in a general cap bundle.

4. **Path resolution ownership is split.** The bridge builds `GadgetPaths`
   from `AppHandle` (platform paths) and its own stored gadget-specific
   paths. The `PathResolverCap` exists but isn't wired into
   `WasmGadgetCaps` or the `paths::resolve` host import because there's
   no clean way to provide it without restructuring provisioning.

5. **`Gadget::provision()` and `type Caps` still exist.** The plan calls
   for removing these and having gadgets receive `Arc<ProvisionedCaps>` at
   construction, but no progress has been made on this.

## Decided Design (from earlier discussion)

### Who builds what

- **GadgetHost** resolves `PlatformPaths` once from `AppHandle` at init.
- **GadgetHost** builds `GadgetPaths` per gadget from `PlatformPaths` +
  gadget-specific paths (`<app_data_dir>/gadget-home/<id>/` and the
  source root). The host already knows the gadget ID (from registration)
  and the source path (from loading).
- **GadgetHost** reads the permission declaration (manifest for WASM,
  trait method / hardcoded for native) and constructs all caps.
- **GadgetHost** assembles `ProvisionedCaps` and hands it to the gadget.
- **`ProvisioningContext`** becomes host-internal — either fields on
  `GadgetHost` itself or a host-internal struct. It does not disappear
  (the host still needs `Store`, `FrecencyStore`, `IconCache`,
  `WebsiteMetadataService` to build caps), but it is never passed to
  gadgets. `AppHandle` never reaches gadget code.
- **Gadgets** receive `Arc<ProvisionedCaps>` at construction. They never
  see raw `AppHandle` or `ProvisioningContext`.

### PathResolver flow

- `GadgetPaths` (per gadget) is passed to cap constructors that need
  variable resolution (`FilesystemCap::new`, `CommandCap::new`) as
  `&impl PathResolver`.
- `PathResolverCap` wraps `GadgetPaths` for runtime resolution via the
  `paths::resolve` WIT host import.
- The bridge receives `PathResolverCap` as part of its caps, never
  builds it.

### Gadget trait changes

- `type Caps` removed — all gadgets receive `Arc<ProvisionedCaps>`.
- `provision()` removed — the host provisions caps externally.
- `AnyGadget` eliminated — `Gadget` trait becomes object-safe.
- `enable()` / `disable()` become pure lifecycle signals.
- `Arc<dyn Gadget>` used directly in `GadgetSlot`.

### Factory-based registration

`requested_caps()` is NOT on the `Gadget` trait — a static method breaks
object-safety, and an instance method can't work because caps must be
built before the gadget is constructed. Cap requests are provided at
registration time via a factory pattern:

```rust
host.register(
    gadget_id,
    cap_requests,
    |caps| MyGadget::new(caps),
    source_kind,
);
```

Native gadgets provide cap requests via an inherent `cap_requests()`
static method. WASM gadgets derive them from the manifest via
`WasmGadgetBridge::cap_requests_from_manifest()`. Gadgets receive
`Arc<ProvisionedCaps>` as a constructor parameter and store it as a
plain field — no `OnceLock`, no `unwrap()`.

### WasmGadgetCaps → ProvisionedCaps merge

Once the host builds caps, `WasmGadgetCaps` should merge into
`ProvisionedCaps`. WASM-specific fields move as follows:

- `gadget_source` → deferred cap (tracked in
  `01kr642tq2r66a9899fw4f0p0p`)
- `gadget_paths` → replaced by `PathResolverCap` in `ProvisionedCaps`
- `sql_handle_reps` → stays on `GadgetState` (bridge-level wasmtime
  resource lifecycle, not a capability)
- `settings` / `frecency` → already on `ProvisionedCaps` (now opt-in
  via `[permissions]`, no longer unconditionally provisioned)

### CapRequest system

Each gadget declares what it needs via `Vec<CapRequest>` provided at
registration time (factory pattern — see "Factory-based registration"
section above). Struct variants maintain an explicit division between
`permissions` (security boundary) and `config` (construction data):

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
        config: SqlStorageConfig,       // migrations
    },
    Clipboard,
    WebsiteMetadata,
    IconCache,
    Settings,
    Frecency,
    PathResolver,
}
```

`*Permissions` and `*Config` are domain types living in `caps/`. Manifest
deserialization structs (`*PermissionsDef`) live in
`wasm/manifest/permissions/` with `Into` impls owned by the manifest
module. Dependency direction: `wasm/manifest/` → `caps/`, never reverse.

Shared domain types like `ArgvConstraint` live in `caps/` as authoritative;
the manifest module has its own serde-decorated mirror type with an `Into`
impl.

For WASM: `WasmGadgetBridge::cap_requests_from_manifest()` converts the
parsed `[permissions]` section through these `Into` conversions.
For native: inherent `cap_requests()` static methods return the list.

### Manifest `[permissions]` structure

`[permissions]` is the single source of truth for what caps a gadget
receives. Configuration sections (`[storage.sql]`, `[settings]`) provide
additional data but do not imply cap provisioning.

Two forms:
- **Flat boolean** for parameterless caps: `clipboard = true`,
  `settings = true`, `frecency = true`, `path-resolver = true`, etc.
- **Subsection** for parameterized caps: `[permissions.opener]`,
  `[permissions.http]`, `[permissions.filesystem]`,
  `[[permissions.command]]`.

Presence of a key or subsection = cap requested. Omission = not requested
(no explicit `= false` needed).

Key naming changes from current format: `fs` → `filesystem`,
`settings` / `frecency` / `sql-storage` / `path-resolver` are new
explicit opt-in entries (previously implicit or unavailable).

Full details and examples in the unified capability plan
(`01kr3w83g1txd12zwapqsddqgv`).

## Implementation Steps

### Phase A: Move cap construction from bridge to host

1. Move `gadget_data` / `gadget_archive` path computation from
   `WasmGadgetBridge::new()` to `GadgetHost` (or its provisioner).
2. Have `GadgetHost` resolve `PlatformPaths` once, build `GadgetPaths`
   per gadget.
3. Move cap construction logic from `WasmGadgetBridge::provision()` into
   a host-level provisioner that reads the manifest and builds
   `ProvisionedCaps`.
4. The bridge receives `ProvisionedCaps` (or `Arc<ProvisionedCaps>`)
   instead of building `WasmGadgetCaps`.
5. Native gadgets receive the same `ProvisionedCaps`.

### Phase B: Restructure the Gadget trait

1. Remove `type Caps` and `provision()`.
2. Change `enable()` to receive `Arc<ProvisionedCaps>` or take no args
   (caps received at construction).
3. Remove `AnyGadget` (trait is now object-safe).
4. Update `GadgetSlot` to hold `Arc<dyn Gadget>` directly.
5. Update all native gadgets and the WASM bridge.

### Phase C: CapRequest and permission declarations

1. Define domain types in `caps/`: `*Permissions` structs (e.g.,
   `OpenerPermissions`, `FilesystemPermissions`), `*Config` structs
   (e.g., `SqlStorageConfig`), and shared types (e.g., `ArgvConstraint`).
2. Define `CapRequest` enum with struct variants using named
   `permissions` / `config` fields.
3. Implement factory-based registration on `GadgetHost` — takes
   `Vec<CapRequest>` + factory closure, builds `ProvisionedCaps`,
   constructs gadget with caps as a plain field.
4. Native gadgets provide cap requests via inherent `cap_requests()`
   static methods. `WasmGadgetBridge` provides them via
   `cap_requests_from_manifest()`. Cap declarations are NOT on the
   `Gadget` trait (static method breaks object-safety, instance method
   can't work because caps must exist before construction).
5. Create serde-decorated manifest mirror types in
   `wasm/manifest/permissions/` with `Into` impls converting to domain
   types. Manifest module owns all conversions.
6. Restructure `PermissionsDef` to match the new manifest format:
   add boolean fields for `settings`, `frecency`, `sql-storage`,
   `path-resolver`, `clipboard`, `icon-cache`; rename `fs` →
   `filesystem`.
7. Host provisioner reads `Vec<CapRequest>`, validates, and provisions
   only requested caps.
8. Manifest validation: warn on contradictions (e.g., `[storage.sql]`
   present without `sql-storage = true`).
9. Update all existing gadget `manifest.toml` files to the new format.

### Phase D: Merge WasmGadgetCaps into ProvisionedCaps

1. Move `sql_handle_reps` to `GadgetState` (bridge-internal).
2. Handle `gadget_source` (deferred cap or bridge-internal).
3. Delete `WasmGadgetCaps`, use `ProvisionedCaps` everywhere.

## Current State (after cap type migration)

- All `*Cap` types exist in `src-tauri/src/caps/` with internal
  permission checking.
- `ProvisionedCaps` struct exists with `cap_accessor!` macro but is
  unused at runtime.
- `WasmGadgetCaps` holds `*Cap` types but is still assembled by the
  bridge.
- `PathResolverCap` exists but is not wired into `WasmGadgetCaps` or
  the `paths::resolve` host import — the host import still accesses
  `caps.gadget_paths` directly.
- `ProvisioningContext` still carries raw `AppHandle`.
- The bridge still holds raw manifest permission data and constructs
  caps itself.

## Dependencies

- `01kr642tq2r66a9899fw4f0p0p` — GadgetSource as a cap
- `01kr65dwvn5ybgajdfjs0s616w` — logging/platform as caps
- `01kr65dwvn5ybgajdfjs0s616x` — assets host import as a cap
