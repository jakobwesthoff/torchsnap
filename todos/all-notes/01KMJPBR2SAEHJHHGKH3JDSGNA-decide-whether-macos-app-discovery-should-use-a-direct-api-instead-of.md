---
title: "Decide whether macOS app discovery should use a direct API instead of mdfind"
kind: investigation
component: platform
status: needs-discussion
---

# Decide whether macOS app discovery should use a direct API instead of mdfind

The app launcher discovers installed applications by running `mdfind`
(the Spotlight CLI). That works, but it spawns a subprocess, and a
2026-07-15 incident showed that it can also return a successful but
incomplete result. This todo compares the direct API options. The choice
is still open.

## Context

The app launcher gadget currently uses `mdfind` (Spotlight CLI) to
discover installed applications. This works but spawns a subprocess. A
direct API call would be faster and cleaner.

## Options investigated (2026-03-25)

1. **`_LSCopyAllApplicationURLs`** (LaunchServices private API)
   - Leading underscore: this is a **private, undocumented** API.
   - Signature: `void _LSCopyAllApplicationURLs(NSArray **outURLs)`
   - Would require raw `unsafe` FFI, and no Rust crate wraps it.
   - Risk: App Store rejection, and potential breakage in future macOS
     releases.
   - Fast: reads from the Launch Services database (cached).

2. **`LSCopyApplicationURLsForBundleIdentifier`** (public API)
   - Available in the `core-services` Rust crate (v1.0.0).
   - Requires knowing the bundle ID upfront, so it is not useful for
     discovery.

3. **`NSMetadataQuery`** (public, programmatic Spotlight)
   - What `mdfind` wraps internally.
   - Async and callback-based, more complex than needed.
   - Available via `objc2` bindings.

4. **Directory scanning** (`std::fs::read_dir`)
   - Walk `/Applications`, `~/Applications` and `/System/Applications`.
   - Simple, fully public, no framework dependencies.
   - Misses apps installed in non-standard locations.

## Current decision

Deferred. The app uses `mdfind` behind an `AppDiscovery` trait
abstraction, so the backend can be swapped later without touching gadget
logic. The trait also serves as the platform abstraction point (Linux:
`.desktop` files, Windows: Start Menu).

## Update (2026-07-15)

A broken Data-volume Spotlight store caused `mdfind` to return only
Safari.app from `/Applications`, while 61 apps were installed there. It
exited 0, so discovery succeeded with silently incomplete data. Details
are in the todo titled "Add a directory-scan fallback when Spotlight
results are implausible". This adds a correctness argument for option 4
(directory scanning) beyond the original performance motivation.

During the same incident, option 1 (`_LSCopyAllApplicationURLs`) was
verified working on macOS 26.5.1 via a Swift probe. It returned 365
registered apps, 177 of them under Applications directories, including
Ghostty.app, which `mdfind` missed while the Spotlight store was corrupt.
The LaunchServices registry is maintained independently of the volume
metadata store, so this backend is immune to that failure mode. The API
remains private and undocumented. Calling it from Rust requires an
`unsafe extern "C"` declaration against CoreServices that returns a
`CFArrayRef` of `CFURLRef`s.
