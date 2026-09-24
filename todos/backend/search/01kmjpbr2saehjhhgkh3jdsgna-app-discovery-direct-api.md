---
kind: decision
status: open
---

# App Discovery: Evaluate Direct API Instead of mdfind

Evaluate replacing the `mdfind` subprocess call with a direct API for
application discovery on macOS.

## Context

The app launcher gadget currently uses `mdfind` (Spotlight CLI) to
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
so the backend can be swapped later without touching gadget logic.
The trait also serves as the platform abstraction point (Linux:
`.desktop` files, Windows: Start Menu).

## Update (2026-07-15)

A broken Data-volume Spotlight store caused `mdfind` to return only
Safari.app from `/Applications` (61 apps installed) while exiting 0,
so discovery succeeded with silently incomplete data. Details in
`01kxk8kxsaed0ah2q0z6eamhg2-app-discovery-fallback-directory-scan.md`.
This adds a correctness argument for option 4 (directory scanning)
beyond the original performance motivation.

During the same incident, option 1 (`_LSCopyAllApplicationURLs`) was
verified working on macOS 26.5.1 via a Swift probe: it returned 365
registered apps (177 under Applications directories, including the
Ghostty.app that `mdfind` missed) while the Spotlight store was
corrupt. The LaunchServices registry is maintained independently of
the volume metadata store, so this backend is immune to the failure
mode above. The API remains private/undocumented; calling it from
Rust requires an `unsafe extern "C"` declaration against
CoreServices returning a `CFArrayRef` of `CFURLRef`s.
