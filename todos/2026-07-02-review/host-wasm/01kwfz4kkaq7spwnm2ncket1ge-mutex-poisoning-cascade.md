# Mutex-poisoning `expect`s turn one panic into a permanent cascade

**Kind:** improvement
**Severity:** medium
**Area:** src-tauri/src/wasm/runtime/instance.rs (pattern is codebase-wide)

## Problem
Every guest-call entry point locks the store with
`self.store.lock().expect("store not poisoned")`
(`src-tauri/src/wasm/runtime/instance.rs:79,134,145,162,190,216,226,265,306`).
The same pattern guards the archive mutex
(`wasm/source.rs:454,474`) and the protocol registry
(`wasm/protocol.rs:140`).

If any host-import implementation panics while a guest call
holds the store lock (host imports execute inside
`call_search`/`call_execute` with the lock held), the mutex is
poisoned. From then on, *every* future call into that gadget
panics at the `expect` — including `disable()`, which means the
gadget cannot even be cleanly shut down or reloaded. One panic
converts a single failed call into a permanently wedged gadget,
and in Tauri command context each of those panics may take the
invoking thread with it.

## Impact
Robustness: a transient bug in one host import (or in
wasmtime-wasi) escalates to a wedged gadget requiring app
restart. Since gadgets are user-installable, "keep the host
alive when a gadget path fails" is a stated design goal that
this pattern undermines.

## Suggested fix
Pick a deliberate poisoning strategy and apply it uniformly:
either recover the guard (`lock().unwrap_or_else(|p|
p.into_inner())` — sound here because `GadgetState` has no
invariant that survives a poisoned write half-done, the
subsequent instance teardown discards it) and mark the instance
dead so the bridge disables the gadget, or switch to
`parking_lot::Mutex` (no poisoning) plus explicit
health-tracking. A repo-wide grep for `not poisoned` shows the
full call-site list.
