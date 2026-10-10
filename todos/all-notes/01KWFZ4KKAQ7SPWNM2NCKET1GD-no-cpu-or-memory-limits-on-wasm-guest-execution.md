---
title: "No CPU or memory limits on WASM guest execution"
kind: improvement
component: gadget-host
---
# No CPU or memory limits on WASM guest execution

Guest code runs without a CPU or memory limit. A guest that loops forever blocks the host thread that called it, and a guest that grows linear memory without bound can exhaust the process. Third-party gadget installation is a supported flow, so an accidental `loop {}` in a gadget is enough to cause either.

## Problem
`WasmRuntime::new` configures the engine with only the component model and the Winch strategy (`src-tauri/src/wasm/runtime/engine.rs`, around lines 60 to 71). `instantiate` creates the `Store` with no limiter (around line 171).

On the CPU side, neither `Config::epoch_interruption` nor fuel is enabled. A guest that enters an infinite loop (through a bug or hostile code) blocks the calling host thread forever. A separate todo covers cancelling blocking *host imports*. Pure guest compute loops need a different mechanism (an epoch deadline plus a ticker thread), and that todo does not cover them.

On the memory side, there is no `Store::limiter` or `StoreLimits`, so a guest can grow linear memory without bound. A separate todo targets idle RSS, not a hard cap against runaway allocation.

## Impact
A single misbehaving gadget can hang whichever host thread called into it (query path, enable, scheduled task), or exhaust process memory. Both are plausible without malice. An accidental `loop {}` in a gadget is enough.

## Proposal
Enable `epoch_interruption`. Set a per-call epoch deadline around every guest invocation, with one background ticker thread for the whole process. Attach a `StoreLimits`-based limiter with a sane per-gadget memory ceiling. Surface both as "gadget misbehaved" errors through the existing gadget-error reporting path.

## Relations
- Relates to: [Add host-side cancellation to blocking host imports](01KQF7H3R969ZKC9G2QMK7FBSR-add-host-side-cancellation-to-blocking-host-imports.md), covers blocking host imports, not guest compute loops.
- Relates to: [Tune wasmtime `Config` for lower per-instance memory](01KQZJDCAK7WZ8CPZSDSZN6C68-tune-wasmtime-config-for-lower-per-instance-memory.md), targets idle RSS, not a hard memory cap.
