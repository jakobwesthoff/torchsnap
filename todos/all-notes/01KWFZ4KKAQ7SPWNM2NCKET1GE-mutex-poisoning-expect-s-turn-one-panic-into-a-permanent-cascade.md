---
title: "Mutex-poisoning `expect`s turn one panic into a permanent cascade"
kind: improvement
component: gadget-host
status: needs-discussion
tags: [concurrency]
---
# Mutex-poisoning `expect`s turn one panic into a permanent cascade

A panic inside a host import can wedge a gadget permanently, because every later call into that gadget panics on a poisoned lock. The pattern is codebase-wide, and the cost is that the gadget cannot even be disabled until the app restarts.

## Problem
Every guest-call entry point locks the store with `self.store.lock().expect("store not poisoned")` (`src-tauri/src/wasm/runtime/instance.rs`, around lines 79 to 306). The same pattern guards the archive mutex (`src-tauri/src/wasm/source.rs`, around lines 454 and 474) and the protocol registry (`src-tauri/src/wasm/protocol.rs`, around line 140).

If any host-import implementation panics while a guest call holds the store lock (host imports execute inside `call_search`/`call_execute` with the lock held), the mutex is poisoned. From then on, every future call into that gadget panics at the `expect`, including `disable()`. That means the gadget cannot even be cleanly shut down or reloaded. One panic converts a single failed call into a permanently wedged gadget. In Tauri command context, each of those panics may take the invoking thread with it.

## Impact
Robustness: a transient bug in one host import (or in wasmtime-wasi) escalates to a wedged gadget that requires an app restart. Since gadgets are user-installable, "keep the host alive when a gadget path fails" is a stated design goal, and this pattern undermines it.

## Proposal
Pick a deliberate poisoning strategy and apply it uniformly. Either:
- recover the guard (`lock().unwrap_or_else(|p| p.into_inner())`) and mark the instance dead so the bridge disables the gadget. This is sound here because `GadgetState` has no invariant that survives a half-finished write, and the subsequent instance teardown discards it; or
- switch to `parking_lot::Mutex` (no poisoning) plus explicit health tracking.

A repo-wide grep for `not poisoned` shows the full call-site list.
