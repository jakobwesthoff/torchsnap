---
kind: bug
severity: low
status: open
area: [src-tauri/src/network/website_metadata/mod.rs]
tags: [unconfirmed]
---

# Metadata lookup: blocking mode can stall ~8s; Cached mode parks a thread per call

## Problem
Two latency/resource behaviors of `lookup` diverge from what
their docs suggest:

1. **Blocking mode stacks sequential timeouts.** The service's
   HTTP client has a 2 s default timeout
   (`src-tauri/src/network/website_metadata/mod.rs:253-257`). A
   cold `LookupMode::Blocking` lookup for an unresponsive-but-
   routable host can serially spend: page fetch (2 s), then —
   when the page yields nothing — `try_fallback_favicon` tries
   `/favicon.svg`, `/favicon.png`, `/favicon.ico` at 2 s each
   (`mod.rs:493-521`), for a worst case around 8 s before the
   caller unblocks. `WebsiteMetadataCap::lookup` wraps this in
   `block_in_place` on a guest call
   (`src-tauri/src/caps/website_metadata.rs:41`), so a gadget's
   search/execute can freeze for that long. The mode's doc says
   "waits until the host has an answer" without bounding the
   wait (`mod.rs:12-16`).

2. **Cached mode is documented as coalescing to a no-op but
   parks a thread.** `lookup(…, Cached)` on a miss spawns
   `spawn_blocking(|| fetch_coalesced(...))` with the comment
   "coalescing makes this a no-op if a leader is already
   running" (`mod.rs:390-399`). It is not a no-op: a
   non-leader `fetch_coalesced` call *parks the blocking-pool
   thread on the condvar* until the leader publishes
   (`mod.rs:573-582`). N Cached lookups for the same slow
   domain during one fetch occupy N blocking-pool threads for
   the full fetch duration. The pool is large, so this is
   waste rather than deadlock, but the comment misleads.

## Impact
Worst-case multi-second gadget stalls on blocking lookups, and
transient blocking-thread pileups on keystroke-driven cached
lookups against slow domains.

## Suggested fix
For (1): give the fallback probes a shared deadline (or reuse
one request budget across the whole `fetch_and_cache_inner`),
and document the bound on `LookupMode::Blocking`. For (2): check
`in_flight` before spawning (skip the spawn when a leader
exists — subscribers gain nothing in Cached mode since nobody
consumes the result), or fix the comment to describe the real
cost.
