---
title: "Add host-side cancellation to blocking host imports"
kind: feature
component: gadget-host
status: needs-discussion
tags: [concurrency, performance]
---
# Add host-side cancellation to blocking host imports

A slow blocking host import holds the gadget instance's mutex for its full duration, so every later keystroke's search queues behind it. This todo captures the full architectural problem and a preliminary design space. The implementation is deferred, because the change is cross-cutting and every aspect needs careful review before code is written.

## Context: the problem

WASM gadget instances are single-threaded by component-model spec. The host's `WasmGadgetInstance` (`src-tauri/src/wasm/runtime/instance.rs`, around lines 41 and 232) serializes calls into each instance with a `Mutex<Store>`. When a gadget makes a host import call that blocks for a significant time, the mutex is held for the entire wait. Example: `website_metadata::lookup` with `LookupMode::Blocking` on a cold cache uses `tokio::task::block_in_place` (`src-tauri/src/wasm/runtime/host/website_metadata.rs`, around line 72) to run a 2+ second synchronous network fetch.

The orchestrator (`GadgetHost::search` in `src-tauri/src/gadget_host.rs`, around lines 449 and 560) dispatches each keystroke's search to all gadgets via `spawn_blocking`. With no preemption, every keystroke during a slow call queues a new task that eventually runs to completion.

A separate, simpler fix (a latest-wins mutex acquire, so older queued calls drop themselves rather than draining the queue) is not yet filed as a todo. It is in scope for short-term implementation and addresses the wasted-compute side of this problem.

What that simpler fix does NOT solve: **the in-flight slow call cannot be preempted.** If the user types URLs faster than each cold-cache lookup completes (plausible at 2 s per query), the open-url gadget stays 2 to 4 seconds behind the user's current query. Each result that arrives is already stale. The frontend's generation check (`src/launcher/hooks/useSearch.ts`, around line 102) correctly discards it, so from the user's perspective the gadget never produces a fresh result during URL-typing bursts.

This todo addresses **option C** from the design discussion: plumb cancellation through every blocking host import, so that when a search becomes stale the in-flight call can be terminated, the mutex released and the next query's lookup started without waiting for the obsolete one to finish.

## Why we need this even if open-url is the only consumer

Open-url's bare-domain validation genuinely requires blocking, because the result's *existence* depends on whether the host can reach the domain. Cached mode would return `Pending` on a cold cache, suppress the entry and not pick up the warmed cache until the user types another character. If the user types a URL and stops typing, the result would never appear. Blocking is the only mode that guarantees the result shows up without further user input.

So `lookup_blocking` is a load-bearing pattern, not an escape hatch. Gadgets that want existence-gated results need it. Cancellation is the price of keeping the pattern usable in hot paths.

## Preliminary design space (each item to be debated in detail)

### Cancellation token plumbing

Every host import that calls `block_in_place` (or any other synchronous wait) would need a cancellation token threaded through. When the orchestrator detects a stale search, it signals the token, and the host import returns early with a `Cancelled` indication.

Open questions:

- **Token granularity.** One token per search? Per gadget per search? Per individual host-import call?
- **Token propagation.** How does a host import reach "its" token? Stored on `GadgetState`? Passed as a parameter (which would require WIT changes)? Pulled from a thread-local?
- **What gets cancelled?** The `block_in_place` itself cannot be cancelled, because the synchronous body keeps running. The underlying operation (the HTTP fetch) must support cancellation, so the token has to reach the operation, not just `block_in_place`.

### Underlying operation cancelability

`WebsiteMetadataService::lookup` ultimately uses `reqwest`, which supports cancelable futures via drop. The path to cancellation is therefore to replace the synchronous `block_in_place(|| service.lookup(...))` with `tokio::select! { result = service.lookup(...) => result, _ = token.cancelled() => return Cancelled }` or an equivalent.

Other host imports (`http::fetch`, `sql::query`, possibly others) have their own semantics. Each needs a cancellation review:

- `http::fetch`: already async-friendly underneath, so a similar pattern works.
- `sql::query`: SQLite queries do not usually take seconds, but they could under contention or for very large result sets. SQLite has interrupt support (`sqlite3_interrupt`). Whether `rusqlite` exposes it cleanly is TBD.
- Future host imports would need cancellation declared as part of the interface contract.

### WIT-level surface

The current `website-metadata` interface returns `result<lookup-result, website-metadata-error>`. Cancellation would need to be expressed in one of these ways:

- **Add a `Cancelled` error variant.** The cleanest option. It forces every gadget to handle it (or to pattern-match everything), and it makes cancellation a first-class WIT concept.
- **Add a separate `lookup-result::cancelled` variant.** This conflates cancellation with the data variants. Less clean.
- **Discard via `Result::Err` with a sentinel.** Hidden and error-prone. Rejected.

The first option seems right but introduces a new mandatory variant for gadget authors to handle. The SDK wrapper could abstract it, folding `Cancelled` into `Metadata::Unreachable` with a log, similar to how `PermissionDenied` and `InvalidDomain` are absorbed today, so gadgets rarely see it directly.

### Orchestrator integration

When does the orchestrator signal cancellation?

- **On every new search call**: signal that any in-flight search for this gadget is stale. Aggressive but simple.
- **On idle-and-replace**: signal only when the user clears the query or significantly changes it. Less aggressive, and it preserves results for "typing more characters of the same domain".
- **Gadget-author-controlled**: the gadget opts into cancelability via the manifest. Heavy.

The first option is the most consistent with the generation-based preemption pattern.

### Behaviour during cancellation

Open behavioural questions:

- **Cache poisoning.** Does a cancelled fetch count as "no data yet" (the next call retries) or as a "negative cache for 30 minutes" (the next call gets a cached empty result)? Almost certainly the former, but the service's current code does not distinguish.
- **In-flight coalescing.** The metadata service coalesces concurrent fetches for the same domain into one. If gadget A's fetch is cancelled but gadget B is awaiting the same fetch, what happens? Probably gadget B's fetch should NOT be cancelled, only gadget A's await. That makes cancellation a per-caller concern, not an underlying-operation concern, which changes the implementation significantly.
- **Ordering with mutex release.** When the host import returns `Cancelled`, the WASM call returns and the mutex releases, so the next search acquires it immediately. But if cancellation arrives mid-fetch, the cancel-and-release path must be quick, with no other cleanup work that itself takes time.

### Gadget-side handling

The SDK wrapper currently has:

```rust
pub enum Metadata {
    Found(CacheEntry),
    NoData,
    Unreachable,
    Pending,
}
```

A `Cancelled` variant could be added explicitly, or absorbed into `Unreachable` (with a log) following the existing error-handling convention. Absorption is simpler for gadget authors but loses information that might help distinguish "we gave up because the user moved on" from "the host could not reach the site". For most gadgets the difference is academic, since both lead to "do not show this result". Lean toward absorption.

## Why this is deferred

- **Cross-cutting cost.** It affects the WIT, host imports, the gadget bridge, the metadata service, the SDK wrapper and gadget authors.
- **Many open questions** above each have several valid answers, and getting any one wrong is expensive to undo.
- **The short-term symptom is addressable** by the simpler per-instance latest-wins mutex pattern (not yet filed as a todo), plus the open-url-specific Blocking-mode UX trade-off: open-url's row lags the user's current query by up to 2 s, but other gadgets render normally, assuming the investigation todo confirms incremental streaming.
- **Whether cancellation is needed at all** depends on whether per-keystroke URL typing is how users actually use the feature. Most URL entry happens by paste or by typing and then pausing. Rapid typing of multiple URLs is uncommon. Measure before investing.

## When to revisit

- A second gadget lands that genuinely needs `lookup_blocking` in a hot path (on every keystroke). The per-instance fix does not help these gadgets.
- User reports of open-url's row visibly lagging during fast typing become common.
- A new host import is proposed that is likely to take seconds (for example an LLM call or a translation API).

Until one of those triggers fires, the per-instance latest-wins fix plus the existing UX should be sufficient.

## References

- `src-tauri/src/gadget_host.rs`, `GadgetHost::search`: the orchestrator's search dispatch.
- `src-tauri/src/wasm/runtime/instance.rs`: the per-instance mutex.
- `src-tauri/src/wasm/runtime/host/website_metadata.rs`: the `block_in_place` call site.
- `src-tauri/src/network/website_metadata/`: the service that performs the actual fetch. It would need a cancellation refactor.
- `gadgets/gadget-sdk/wit/torchsnap-gadget.wit`, the `website-metadata` interface: would need a `Cancelled` variant.
- `gadgets/gadget-sdk/src/website_metadata.rs`: the SDK wrapper. The `Metadata` enum may grow a `Cancelled` arm or absorb it.
- `src/launcher/hooks/useSearch.ts`: the frontend generation check. It would still discard stale results, but cancellation prevents the wasted backend work.

## Relations

- Relates to: [Find out whether a slow gadget blocks the whole result list or only its own row](01KQF7H3R969ZKC9G2QMK7FBSQ-find-out-whether-a-slow-gadget-blocks-the-whole-result-list-or-only-its.md), which asks whether the empty-list symptom is per-row or pipeline-wide.
