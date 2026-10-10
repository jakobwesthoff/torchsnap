---
title: "CachedComponent re-acquire trusts a cache file that can vanish or go stale"
kind: bug
component: gadget-host
---
# CachedComponent re-acquire trusts a cache file that can vanish or go stale

Re-acquiring a `CachedComponent` after release assumes its cache file still exists and still matches the source. If the file is removed, every later acquire fails and the gadget stays wedged until app restart, although it could be recompiled. For a dev `DirectorySource` gadget, a rebuilt `.wasm` is silently ignored after an idle release and re-acquire cycle.

## Problem
Read in the code, not reproduced. After the first acquire, `acquire_inner` takes the fast path for all later acquires (`src-tauri/src/wasm/runtime/cached_component.rs`, around lines 203 to 211):

```rust
Some(resolved) => {
    // Re-acquire after release: cache file is
    // guaranteed to exist from the first acquire.
    let c = self.runtime.deserialize_component(&resolved.cache_path)
        .context("re-acquire component from cache")?;
```

The "guaranteed to exist" assumption fails in two ways.

The file can be gone. Anything that clears `<gadget-home>/<id>/compile-cache/` between `release()` and re-acquire makes every future acquire fail with a deserialize error. Possible causes include user cleanup, a second `CachedComponent` for the same gadget pruning to a different filename, or disk restoration. There is no fallback to the `first_acquire` compile path, so the gadget is wedged until app restart even though all inputs to recompile it are still available.

Dev sources go stale. `resolved.cache_path` pins the blake3 hash of the WASM bytes read at first acquire. For a `DirectorySource` dev gadget, rebuilding the `.wasm` and then triggering a release and re-acquire cycle (for example idle eviction, see Relations) silently reloads the old compiled component. The developer's new build is ignored until the `CachedComponent` itself is reconstructed.

A minor related note: the engine-compat half of the cache filename comes from `std::collections::hash_map::DefaultHasher` (`src-tauri/src/wasm/runtime/engine.rs`, around lines 105 to 111). Its output is not guaranteed stable across Rust releases. The failure direction is safe (a spurious recompile after a toolchain bump), so this is only worth fixing opportunistically alongside the above.

## Impact
The first case turns a recoverable cache miss into a persistent gadget load failure. The second produces confusing stale-code behavior in exactly the workflow (dev iteration) that the directory source exists for.

## Suggested fix
On a re-acquire deserialize failure, fall back to the `first_acquire` path instead of propagating the error. For dev (`DirectorySource`) gadgets, skip the `resolved` fast path, or re-hash the wasm bytes when the file mtime changed.

## Relations
- Relates to: [Evict idle gadget instances when the launcher is hidden](01KQZ0FRSMRSVMMNJ6QCM9CBJQ-evict-idle-gadget-instances-when-the-launcher-is-hidden.md), idle eviction triggers the release and re-acquire cycle described above.
