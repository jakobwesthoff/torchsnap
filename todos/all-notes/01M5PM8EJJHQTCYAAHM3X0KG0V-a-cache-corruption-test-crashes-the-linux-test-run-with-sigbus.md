---
title: "A cache corruption test crashes the Linux test run with SIGBUS"
kind: bug
component: gadget-host
impact: high
origin: implementation
tags: [testing, linux]
---
# A cache corruption test crashes the Linux test run with SIGBUS

On Linux the `src-tauri` lib test binary dies with SIGBUS on every run, so the rest of the suite never reports. Found on 2026-10-24 in the Linux arm64 build container (`tools/linux-build`, branch `linux-testbed` at 960535de) while working on the launcher's WebKit cache model. With `--test-threads=1` the last test started before the signal is `wasm::runtime::cached_component::tests::corrupt_cache_file_triggers_recompile`. It happened in six of six runs, with and without other changes, and with `--shm-size=2g` (the container's `/dev/shm` already has 5.9 GiB), so it is not shared memory. macOS runs the same test without a crash.

## Problem

The cause is read in the code, not proven by a run. A cache hit deserializes the compiled component from its `.cwasm` file as a file-backed (mmap'd) component (`src-tauri/src/wasm/runtime/cached_component.rs`, module comment). In `corrupt_cache_file_triggers_recompile` (around line 553) the first `CachedComponent`, `cached`, still holds its component when the test overwrites the cache file in place:

```rust
cached.acquire().expect("first acquire");
// ...
std::fs::write(&cache_path, b"garbage").expect("corrupt file");
```

`fs::write` truncates the same inode to 7 bytes, so the pages `cached` has mapped now lie past the end of the file. On Linux, touching such a page raises SIGBUS, for example when `cached` is dropped or its code is used. The similar test around line 1074 drops its warm-up `CachedComponent` before it corrupts the file, and does not crash.

## Impact

The Linux test run cannot complete, so no Linux run of `just test-crates` (in the container or later in CI) reports the tests after the crash. Until it is fixed, Linux runs have to skip this test.

The app itself writes the cache through a temporary file and `rename` (line 306 to 308), which creates a new inode and leaves existing mappings intact. A crash in the app needs the file to be truncated or rewritten in place by something else while it is mapped, for example a user or a tool emptying `compile-cache/`. Not checked: whether wasmtime or the app guard against that.

## Suggested fix

Drop `cached` before corrupting the file, or corrupt it by writing a new file and renaming it over the cache entry, as the app does. Then run the lib suite in the container without the skip to confirm the cause.

## Relations

- Relates to: [CachedComponent re-acquire trusts a cache file that can vanish or go stale](01KWFZ4KKAQ7SPWNM2NCKET1GF-cachedcomponent-re-acquire-trusts-a-cache-file-that-can-vanish-or-go.md)
