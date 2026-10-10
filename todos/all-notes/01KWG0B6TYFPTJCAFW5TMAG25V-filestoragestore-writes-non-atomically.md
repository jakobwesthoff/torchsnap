---
title: "FileStorage::store writes non-atomically"
kind: bug
component: backend
impact: low
---
# FileStorage::store writes non-atomically

`FileStorage::store` in `src-tauri/src/storage/file_storage.rs` writes blobs directly to their final path. A crash or power loss mid-write leaves a truncated file there, and later `load` calls return the corrupt bytes as valid, because the key hashes the lookup input and not the content. Read in the code, not reproduced.

## Problem
`FileStorage::store` writes blobs with a direct `fs::write`
(`src-tauri/src/storage/file_storage.rs:118-124`). A crash or
power loss mid-write leaves a truncated file at the final path,
and every later `load` happily returns the corrupt bytes. The
key is a hash of the *lookup input* (e.g. a domain), not of the
content, so corruption is undetectable.

Contrast `CachedComponent::first_acquire`
(`src-tauri/src/wasm/runtime/cached_component.rs:303-307`), which writes to a
`.tmp` sibling and renames, the established pattern in this
codebase for crash-safe writes.

## Impact
For current consumers (favicon store) the damage is a corrupt
icon rendered until the cache entry is evicted/refreshed. Any
future consumer with more valuable blobs inherits the same
weakness silently.

## Suggested fix
Write to `<path>.tmp` then `fs::rename` into place, mirroring
the compile-cache implementation.
