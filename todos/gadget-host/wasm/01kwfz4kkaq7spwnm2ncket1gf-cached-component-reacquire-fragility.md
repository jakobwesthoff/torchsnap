# CachedComponent re-acquire trusts a cache file that can vanish or go stale

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/wasm/runtime/cached_component.rs

## Problem
After the first acquire, `acquire_inner` takes the fast path for
all later acquires
(`src-tauri/src/wasm/runtime/cached_component.rs:203-211`):

```rust
Some(resolved) => {
    // Re-acquire after release: cache file is
    // guaranteed to exist from the first acquire.
    let c = self.runtime.deserialize_component(&resolved.cache_path)
        .context("re-acquire component from cache")?;
```

Two problems with the "guaranteed to exist" assumption:

1. **The file can be gone.** Anything that clears
   `<gadget-home>/<id>/compile-cache/` between `release()` and
   re-acquire (user cleanup, a second `CachedComponent` for the
   same gadget pruning to a different filename, disk
   restoration) makes every future acquire fail with a
   deserialize error. There is no fallback to the
   `first_acquire` compile path, so the gadget is wedged until
   app restart even though all inputs to recompile it are still
   available.
2. **Dev sources go stale.** `resolved.cache_path` pins the
   blake3 hash of the WASM bytes read at first acquire. For a
   `DirectorySource` dev gadget, rebuilding the `.wasm` and then
   triggering a release/re-acquire cycle (e.g. idle eviction,
   which `todos/gadget-host/memory/01kqz0frsmrsvmmnj6qcm9cbjq-idle-instance-eviction.md`
   plans) silently reloads the OLD compiled component; the
   developer's new build is ignored until the
   `CachedComponent` itself is reconstructed.

Minor related note: the engine-compat half of the cache filename
is produced with `std::collections::hash_map::DefaultHasher`
(`:105-111` in engine.rs), whose output is not guaranteed stable
across Rust releases. Failure direction is safe (spurious
recompile after a toolchain bump), so this is only worth fixing
opportunistically alongside the above.

## Impact
Case 1 turns a recoverable cache miss into a persistent gadget
load failure. Case 2 produces confusing stale-code behavior in
exactly the workflow (dev iteration) the directory source
exists for.

## Suggested fix
On re-acquire deserialize failure, fall back to the
`first_acquire` path instead of propagating the error. For dev
(`DirectorySource`) gadgets, skip the `resolved` fast path or
re-hash the wasm bytes when the file mtime changed.
