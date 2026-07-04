# No CPU or memory limits on WASM guest execution

**Kind:** improvement
**Severity:** medium
**Area:** src-tauri/src/wasm/runtime/engine.rs

## Problem
`WasmRuntime::new` configures the engine with only the component
model and the Winch strategy
(`src-tauri/src/wasm/runtime/engine.rs:60-71`); `instantiate`
creates the `Store` with no limiter (`:171`). Consequently:

- **CPU:** neither `Config::epoch_interruption` nor fuel is
  enabled, so a guest that enters an infinite loop (bug or
  hostile) blocks the calling host thread forever. The existing
  todo
  `todos/gadget-host/wasm/01kqf7h3r969zkc9g2qmk7fbsr-host-cancellation-system-for-blocking-imports.md`
  covers cancelling blocking *host imports*; pure guest compute
  loops are a separate mechanism (epoch deadline + a ticker
  thread) and are not covered there.
- **Memory:** no `Store::limiter` / `StoreLimits`, so a guest
  can grow linear memory without bound. The
  `todos/memory/01kqzjdcak7wz8cpzsdszn6c68-wasmtime-memory-config-tuning.md`
  todo targets idle RSS, not a hard cap against runaway
  allocation.

## Impact
A single misbehaving gadget can hang whichever host thread
called into it (query path, enable, scheduled task) or exhaust
process memory. With third-party gadget installation being a
supported flow, both are plausible without malice — an
accidental `loop {}` in a gadget is enough.

## Suggested fix
Enable `epoch_interruption`, set a per-call epoch deadline
around every guest invocation (one background ticker thread
process-wide), and attach a `StoreLimits`-based limiter with a
sane per-gadget memory ceiling. Surface both as "gadget
misbehaved" errors through the existing gadget-error reporting
path.
