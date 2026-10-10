---
title: "Blocking metadata lookups can stall about 8 seconds and cached lookups park threads"
kind: bug
component: backend
impact: low
tags: [performance]
---
# Blocking metadata lookups can stall about 8 seconds and cached lookups park threads

Two latency and resource behaviors of `lookup` in the website-metadata service differ from what the docs suggest. A blocking lookup for an unresponsive host can freeze the gadget call that waits on it for about 8 seconds. A cached lookup that misses parks a blocking-pool thread for each concurrent caller, although its comment calls the extra work a no-op.

## Problem

Read in the code; not confirmed at runtime.

1. **Blocking mode stacks sequential timeouts.** The service's HTTP client has a 2 s default timeout (`src-tauri/src/network/website_metadata/mod.rs`). A cold `LookupMode::Blocking` lookup for an unresponsive but routable host can serially spend: the page fetch (2 s), then, when the page yields nothing, `try_fallback_favicon` tries `/favicon.svg`, `/favicon.png` and `/favicon.ico` at 2 s each. That is a worst case of around 8 s before the caller unblocks. `WebsiteMetadataCap::lookup` in `src-tauri/src/caps/website_metadata.rs` wraps this in `block_in_place` on a guest call, so a gadget's search or execute can freeze for that long. The mode's doc says "waits until the host has an answer" without bounding the wait.

2. **Cached mode is documented as coalescing to a no-op, but it parks a thread.** `lookup(…, Cached)` on a miss spawns `spawn_blocking(|| fetch_coalesced(...))` with the comment "coalescing makes this a no-op if a leader is already running". It is not a no-op. A non-leader `fetch_coalesced` call parks the blocking-pool thread on the condvar until the leader publishes. N Cached lookups for the same slow domain during one fetch occupy N blocking-pool threads for the full fetch duration. The pool is large, so this is waste rather than deadlock, but the comment misleads.

## Impact

Worst-case multi-second gadget stalls on blocking lookups, and transient blocking-thread pile-ups on keystroke-driven cached lookups against slow domains.

## Suggested fix

For (1): give the fallback probes a shared deadline, or reuse one request budget across the whole `fetch_and_cache_inner`, and document the bound on `LookupMode::Blocking`.

For (2): check `in_flight` before spawning, and skip the spawn when a leader exists. Subscribers gain nothing in Cached mode, since nobody consumes the result. Alternatively, fix the comment to describe the real cost.
