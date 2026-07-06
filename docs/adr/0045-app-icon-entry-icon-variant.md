# 45. App-Icon Entry Icon Variant

Date: 2026-07-06

## Status

Accepted

## Context

The WIT `entry-icon` variant (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`)
offers gadgets four icon sources: `hero-icon`, `data-url`, `asset-icon` (a
path relative to the gadget archive root), and `emoji`.

The host's response pass (`resolve_entry_icon` in
`src-tauri/src/wasm/bindings.rs`) rewrites gadget-relative `asset-icon` paths
to `torchsnap-gadget://` URLs, passes host-issued `torchsnap-favicon://` URLs
through unchanged, and drops absolute filesystem paths and other qualified
URLs with a warning routed to the gadget's log via `classify_gadget_asset_path`.
A WASM gadget therefore has no way to reference an icon that lives on the
host filesystem.

The manifest permission `permissions.icon-cache`
(`src-tauri/src/wasm/manifest/permissions/mod.rs`) already provisions
the shared `IconCache` to WASM gadget instances
(`src-tauri/src/wasm/bridge.rs`, `src-tauri/src/gadget_host.rs`),
but no guest-facing WIT surface consumes it. No gadget sets the flag today.

The native app-launcher gadget already extracts real application icons
through this cache: `nsworkspace_icon_for_file`
(`src-tauri/src/platform/macos/cgimage_conversion.rs`) calls
`NSWorkspace::sharedWorkspace().iconForFile(...)` and converts the resulting
`CGImage`, and `extract_icons` in `src-tauri/src/gadgets/app_launcher.rs`
feeds that closure into `IconCache::ensure_icon`
(`src-tauri/src/icons/icon_cache.rs`) keyed by `"{path}:{bundle_id}"`.
`ensure_icon` is a per-gadget-scoped WebP disk cache with mtime-based
staleness checking; it only invokes the image-producing closure on a cache
miss or stale entry. The resulting absolute path is rendered by the frontend
via Tauri's `convertFileSrc` (`src/components/Icon.tsx`).

The awake gadget hardcodes `EntryIcon::HeroIcon("bolt")` on every entry
(`gadgets/awake/src/lib.rs`) even though it drives Amphetamine.app, an
application with its own real icon on the system.

## Decision

### New WIT variant

Add a fifth case to `entry-icon`: `app-icon(string)`. The payload is a
platform-native application identifier; on macOS this is a bundle
identifier (e.g. `com.if.Amphetamine`). The variant is documented in the
WIT from the gadget author's perspective: pass an application identifier to
show that application's real icon.

### Host resolution

The variant is resolved inside the existing response pass alongside
`resolve_entry_icon`, not through a new WIT interface or a new interface
gate. Resolution is gated by the existing `icon-cache` permission — a
gadget must already set `permissions.icon-cache = true` for `app-icon` to
resolve.

On macOS, resolution proceeds: bundle identifier to application path via
`NSWorkspace`'s `URLForApplicationWithBundleIdentifier` (LaunchServices; the
binding is available in the pinned `objc2-app-kit` 0.3.2,
`src-tauri/Cargo.toml`), then icon extraction via
`nsworkspace_icon_for_file`, then caching through `IconCache::ensure_icon`
under the requesting gadget's own cache scope. The resolved entry icon is
emitted as an `asset-icon` carrying the absolute cached path, the same form
the native app-launcher gadget already produces for its own icons. Because
this path is host-resolved rather than gadget-supplied, it is not subject
to the relative-path validation `classify_gadget_asset_path` applies to
gadget-authored `asset-icon` values.

A first-time request for a given bundle identifier extracts the icon on
demand through `ensure_icon`'s lazy closure. The cache's mtime check
re-extracts if the target application changes on disk.

Host-side conversion between the WIT `entry-icon` type and the native
`EntryIcon` type is an exhaustive match (`src-tauri/src/wasm/bindings.rs`);
adding the variant requires a new arm on both directions of that
conversion.

### Failure mode

If the `icon-cache` permission is absent, the identifier does not resolve
to an installed application, icon extraction fails, or the host platform is
not macOS, the icon is dropped and a warning is logged to the gadget's log
on every occurrence. This is the same behavior class the sanitizer already
applies to invalid `asset-icon` paths.

No information about resolution outcome flows back to the guest: a gadget
cannot observe whether an `app-icon` lookup succeeded, so the variant
cannot be used to probe which applications are installed on the host.

### Bundle identifier as the lookup key

The macOS lookup key is the bundle identifier, not the display name.
Display names are non-unique and localized, while a bundle identifier
resolves deterministically to an installed application regardless of
install location through a single LaunchServices call. Apple deprecated
the name-based `NSWorkspace` lookup (`fullPathForApplication`) in favor of
the bundle-identifier API. The codebase already keys application identity
on bundle id elsewhere: the app-launcher icon cache key is
`"{path}:{bundle_id}"` (`src-tauri/src/gadgets/app_launcher.rs`),
and settings discovery uses the bundle id as the settings entry id
(`src-tauri/src/platform/macos/settings_discovery.rs`).

### Rejected alternative

A capability-call interface modeled on `website-metadata::lookup` — a
guest-callable `lookup(id) -> option<entry-icon>` — was considered and
rejected. That shape would send the resolved icon reference from host to
guest and back to the host on render, requiring a new whitelisted URL
scheme in the response sanitizer. It would also let a gadget observe
whether a given application is installed by inspecting the lookup result,
which the chosen design avoids entirely.

### No frontend change

The frontend already renders absolute-path `asset-icon` values through
`convertFileSrc`; the new variant resolves to that same shape before it
ever reaches the frontend, so `src/components/Icon.tsx` is unchanged.

### First consumer

The awake gadget switches every entry except its error entry from the
`bolt` hero-icon to `app-icon` with the bundle identifier
`com.if.Amphetamine`.

## Consequences

Gadgets that wrap a single native macOS application can show that
application's real icon by emitting a bundle identifier string; they never
handle image bytes or filesystem paths directly.

The `icon-cache` permission, previously provisioned but unconsumed, becomes
the opt-in gate for this variant.

The `entry-icon` variant list grows to five cases. The exhaustive
matches converting between the WIT and native `EntryIcon` types in
`src-tauri/src/wasm/bindings.rs` must handle the new case.

Resolution semantics exist only on macOS. On other platforms `app-icon`
currently always drops with a warning.
