---
kind: investigation
status: open
---

# 0.9.2 does not give back 5 to 9 MB that 0.9.1 reclaims late in the benchmark

Measured 2026-09-21, cause unknown.

## Observation

`tools/bench-memory` (55-second scenario, 200 ms sampling, phase
snapshots) was run on 2026-09-21 for both releases, back to back on the
same Mac and profile: one discarded warm-up run per version (switching
versions invalidates the gadget compile cache and the first run
recompiles every gadget, about +60 MB transiently), then two measured
runs each.

Torchsnap process footprint per phase snapshot, MB:

```
                1     2     3     4     5     6     7     8     9    10    11    12    13    14    15
0.9.1 run1    41.5  42.7  42.6  43.4  43.0  42.6  42.7  43.0  42.7  38.1  38.1  38.1  38.2  33.0  33.0
0.9.1 run2    41.5  42.5  42.4  43.3  43.3  43.0  43.1  43.4  43.0  41.7  41.7  41.7  41.7  33.2  33.2
0.9.2 run1    41.6  42.7  42.5  43.3  42.9  42.6  42.8  42.9  42.5  39.1  39.1  39.1  38.9  39.1  39.1
0.9.2 run2    41.6  42.7  42.2  42.9  42.5  42.3  42.4  42.5  42.2  42.4  42.5  42.5  42.1  42.4  42.4
```

Phases, in the order of `tools/bench-memory`: app started (idle),
launcher visible, after first dismiss, query Safari, query Terminal,
after app search dismiss, query `:rocket`, query `:fire`, after emoji
dismiss, query `uni`, query `able`, after hello-world search dismiss,
after 5 rapid cycles, query `settings`, final idle.

- Phases 1 to 9 match within ±0.5 MB. Phases 10 to 13 vary by run
  (38 to 42 MB) on both versions.
- In both 0.9.1 runs the process drops to about 33 MB between "after 5
  rapid cycles" and "query: settings" (about 45 s in). In both 0.9.2 runs
  it stays at 39 to 42 MB until the scenario ends at 55 s.
- Averaged over the run: +3.0 MB (+7.7 %) for the torchsnap process,
  +3.2 MB (+2.2 %) over all processes. Peaks are equal (43.4 vs 43.6 MB).
  WebKit processes show no change beyond noise.
- Noise reference: two August runs of one build differed by 1 to 3 %, up
  to about 8 % in single phases.

## What it is not

The host does not release compiled gadgets on a timer.
`CachedComponent::release()` runs only when a gadget is disabled
(`src-tauri/src/wasm/bridge.rs`, `disable()`), and the scenario is the
same for both versions. The 0.9.1 drop is therefore memory the
allocator or the OS reclaims on its own.

## Differences between the two builds

0.9.2 changed, among others: wasmtime/wasmtime-wasi 47.0.3 -> 49.0.0
(Winch), Rust 1.95.0 -> 1.98.1, Tauri 2.11.5 -> 2.11.6, reqwest/rustls/h2
patch updates, React 19.2 -> 19.3 (webview only). Candidates for a
changed reclamation pattern: wasmtime 49's memory handling (linear
memory, mmap and madvise behaviour) and the Rust 1.98 standard library.

## Next steps

1. Find out whether 0.9.2 reclaims later or holds the memory: extend the
   scenario with several minutes of idle sampling (`tools/memsnap sample`)
   and run it for both versions.
2. If it holds the memory: bisect by building 0.9.2 with wasmtime 47.0.4
   and 48.0.2 (the update was committed one major at a time), and 0.9.1's
   dependencies with Rust 1.98.1, to separate runtime from toolchain.
3. Check wasmtime's changelog for 48/49 memory or allocator changes, and
   see `01kqzjdcak7wz8cpzsdszn6c68-wasmtime-memory-config-tuning.md`
   and `01kqz0frsmrsvmmnj6qcm9cbjq-idle-instance-eviction.md` in this
   directory, which touch the same area.

Run the benchmark with the installed Torchsnap quit; `tools/bench-memory`
stops it and does not restart it.
