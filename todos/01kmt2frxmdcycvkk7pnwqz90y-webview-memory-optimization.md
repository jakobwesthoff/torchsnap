# Webview Memory Optimization for Launcher Window

## Status

Two optimizations implemented and benchmarked (ADR 0020):

1. **Window sized to content** — 808×830 logical px instead of full screen
2. **Shrink to 1×1 on hide** — WebKit releases backing stores between shows

Total idle footprint reduced from ~218 MB to ~107 MB (~50%). WebContent
process alone dropped from 148 MB to 39 MB at idle.

## Remaining Techniques (not yet needed)

The following were identified during investigation but deferred given the
results achieved. They remain available if the residual ~39 MB idle
footprint or the ~55 MB average active footprint become a concern:

- **Destroy-on-hide / create-on-show** — maximum savings but adds
  200-500ms+ show latency
- **Navigate to `about:blank` while hidden** — releases JS heap and DOM
  but still has re-navigation cost
- **Visibility-based DOM cleanup** — unmount heavy React components on
  blur, limited savings since JS heap remains
- **Hybrid idle timer** — destroy after N seconds of inactivity, fast
  re-show within grace period

## Measurement Tooling

Shell-based tooling in `tools/` (not a Rust CLI as originally planned):

- `tools/memsnap sample` — continuous `phys_footprint` sampling via
  macOS `footprint` tool with coalition-based process discovery
- `tools/memsnap snapshot` — single-shot labeled measurements for
  phase-by-phase comparison
- `tools/bench-memory` — full lifecycle orchestrator (build → launch →
  scenario → kill → report)
- `tools/memsnap-report` — ASCII table formatting for benchmark log

Benchmark results are appended to `memory-benchmark.log`.
