# App Discovery: Evaluate Direct API Instead of mdfind

Evaluate replacing the `mdfind` subprocess call with a direct API for
application discovery on macOS.

## Context

The app launcher plugin currently uses `mdfind` (Spotlight CLI) to
discover installed applications. This works but spawns a subprocess.
A direct API call would be faster and cleaner.

## Options investigated (2026-03-25)

1. **`_LSCopyAllApplicationURLs`** (LaunchServices private API)
   - Leading underscore — this is a **private, undocumented** API
   - Signature: `void _LSCopyAllApplicationURLs(NSArray **outURLs)`
   - Would require raw `unsafe` FFI, no Rust crate wraps it
   - Risk: App Store rejection, potential breakage in future macOS
   - Fast: reads from Launch Services database (cached)

2. **`LSCopyApplicationURLsForBundleIdentifier`** (public API)
   - Available in `core-services` Rust crate (v1.0.0)
   - Requires knowing bundle ID upfront — not useful for discovery

3. **`NSMetadataQuery`** (public, programmatic Spotlight)
   - What `mdfind` wraps internally
   - Async/callback-based, more complex than needed
   - Available via `objc2` bindings

4. **Directory scanning** (`std::fs::read_dir`)
   - Walk `/Applications`, `~/Applications`,
     `/System/Applications`
   - Simple, fully public, no framework dependencies
   - Misses apps installed in non-standard locations

## Decision

Deferred. Using `mdfind` behind an `AppDiscovery` trait abstraction
so the backend can be swapped later without touching plugin logic.
The trait also serves as the platform abstraction point (Linux:
`.desktop` files, Windows: Start Menu).
