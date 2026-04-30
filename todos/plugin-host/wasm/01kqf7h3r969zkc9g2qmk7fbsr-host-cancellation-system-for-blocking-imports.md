# Host-side cancellation system for blocking host imports

## Status: requires major design discussion before implementation

This todo captures the *full architectural problem* and a
preliminary design space, but the actual implementation is
deferred. The cross-cutting nature of the change means every
aspect needs careful review before code is written.

## Context: the problem

WASM plugin instances are single-threaded by component-model
spec. The host's `WasmPluginInstance` (`src-tauri/src/wasm/runtime/instance.rs:41,232`)
serializes calls into each instance with a `Mutex<Store>`.
When a plugin makes a host import call that blocks for a
significant time — e.g. `website_metadata::lookup` with
`LookupMode::Blocking` on a cold cache, which uses
`tokio::task::block_in_place` (`src-tauri/src/wasm/runtime/host/website_metadata.rs:72`)
to run a 2+ second synchronous network fetch — the mutex is
held for the entire fetch duration.

The orchestrator (`PluginHost::search` at
`src-tauri/src/plugin_host.rs:449,560`) dispatches each
keystroke's search to all plugins via `spawn_blocking`. With
no preemption, every keystroke during a slow call queues a
new task that eventually runs to completion.

A separate, simpler todo
(`<TBD-once-A-prime-todo-exists>`) addresses the wasted-
compute side of this — "latest-wins" mutex acquire so older
queued calls drop themselves rather than draining the queue.
That fix is in scope for short-term implementation.

What that simpler fix does NOT solve: **the in-flight slow
call cannot be preempted.** If the user is typing URLs faster
than each cold-cache lookup completes (which is plausible at
2s/query), the open-url plugin is permanently 2-4 seconds
behind the user's current query. Each result that arrives is
already stale (correctly discarded by the frontend's
generation check at `src/launcher/hooks/useSearch.ts:102`),
so from the user's perspective the plugin appears to never
produce a fresh result during URL-typing bursts.

This todo addresses **option C** from the design discussion:
plumb cancellation through every blocking host import so that
when a search becomes stale, the in-flight call can be
terminated, the mutex released, and the next query's lookup
started without waiting for the obsolete one to finish.

## Why we need this even if open-url is the only consumer

Open-url's bare-domain validation logic genuinely requires
blocking — the result's *existence* depends on whether the
host can reach the domain. Cached mode would return `Pending`
on cold cache, suppress the entry, and not pick up the warmed
cache until the user types another character. If the user
types a URL and stops typing, the result would never appear.
Blocking is the only mode that guarantees the result shows up
without further user input.

So `lookup_blocking` is a load-bearing pattern, not an escape
hatch. Plugins that want existence-gated results need it.
Cancellation is the price for keeping the pattern usable in
hot paths.

## Preliminary design space (each item to be debated in detail)

### Cancellation token plumbing

Every host import that calls `block_in_place` (or any other
synchronous wait) would need a cancellation token threaded
through. When the orchestrator detects a stale search, it
signals the token; the host import returns early with a
`Cancelled` indication.

Open questions:

- **Token granularity.** One token per search? Per plugin per
  search? Per individual host-import call?
- **Token propagation.** How does a host import access "its"
  token? Stored on `PluginState`? Passed as a parameter (would
  require WIT changes)? Pulled from a thread-local?
- **What gets cancelled?** The `block_in_place` itself can't
  be cancelled — the synchronous body keeps running. The
  underlying operation (the HTTP fetch) needs to support
  cancellation. So the token has to reach the operation, not
  just `block_in_place`.

### Underlying operation cancelability

`WebsiteMetadataService::lookup` ultimately uses `reqwest`,
which supports cancelable futures via dropping. So the path
to cancellation is: replace the synchronous `block_in_place(||
service.lookup(...))` with `tokio::select! { result =
service.lookup(...) => result, _ = token.cancelled() =>
return Cancelled }` or equivalent.

But other host imports (`http::fetch`, `sql::query`, possibly
others) have their own semantics. Each needs a cancellation
review:

- `http::fetch` — already async-friendly underneath, similar
  pattern works.
- `sql::query` — SQLite queries don't usually take seconds,
  but they could under contention or for very large result
  sets. SQLite has interrupt support (`sqlite3_interrupt`);
  whether `rusqlite` exposes it cleanly is TBD.
- Future host imports — would need cancellation as a
  declared part of the interface contract.

### WIT-level surface

The current `website-metadata` interface returns
`result<lookup-result, website-metadata-error>`. Cancellation
would need to either:

- **Add a `Cancelled` error variant.** Cleanest. Forces
  every plugin to handle it (or pattern-match-all). Plumbs
  cancellation as a first-class WIT concept.
- **Add a separate "lookup-result::cancelled" variant.**
  Conflates with the data variants. Less clean.
- **Discard via Result::Err with a sentinel.** Hidden, error-
  prone. Reject.

The first option seems right but introduces a new mandatory
variant for plugin authors to handle. The SDK wrapper could
abstract it (fold `Cancelled` into `Metadata::Unreachable`
with a log, similar to how `PermissionDenied` and
`InvalidDomain` are absorbed today) so plugins typically
never see it directly.

### Orchestrator integration

When does the orchestrator signal cancellation?

- **On every new search call:** signal "any in-flight search
  for this plugin is stale." Aggressive but simple.
- **On idle-and-replace:** signal only when the user clears
  or significantly changes the query. Less aggressive,
  preserves results for "typing more characters of the same
  domain."
- **Plugin-author-controlled:** plugin opts into cancelability
  via manifest. Heavy.

The first option is most consistent with the generation-based
preemption pattern.

### Behaviour during cancellation

Open behavioural questions:

- **Cache poisoning.** A cancelled fetch — does the metadata
  service treat it as "no data yet" (next call retries) or as
  "negative cache for 30 minutes" (next call gets cached
  empty)? Almost certainly the former, but the service's
  current code doesn't distinguish.
- **In-flight coalescing.** The metadata service coalesces
  concurrent fetches for the same domain into one. If plugin
  A's fetch is cancelled but plugin B is awaiting the same
  fetch, what happens? Probably plugin B's fetch should NOT
  be cancelled, only plugin A's await — which means
  cancellation is at the per-caller layer, not the underlying
  operation layer. That changes the implementation
  significantly.
- **Ordering with mutex release.** When the host import
  returns `Cancelled`, the WASM call returns, the mutex
  releases. The next search call acquires immediately. Good.
  But if the cancellation arrives mid-fetch, the cancel-and-
  release path needs to be quick — no other "cleanup" work
  that itself takes time.

### Plugin-side handling

The SDK wrapper currently has:

```rust
pub enum Metadata {
    Found(CacheEntry),
    NoData,
    Unreachable,
    Pending,
}
```

A `Cancelled` variant could be added explicitly, OR
absorbed into `Unreachable` (with a log) following the
existing error-handling convention. The latter is simpler for
plugin authors but loses information that *might* be useful
for distinguishing "we tried and gave up because user moved
on" from "host couldn't reach."

For most plugins, the difference is academic — both lead to
"don't show this result." Lean toward absorption.

## Why this is deferred

- **Cross-cutting cost.** Affects WIT, host imports, plugin
  bridge, metadata service, SDK wrapper, plugin authors.
- **Many open questions** above each have multiple valid
  answers; getting any one wrong is expensive to undo.
- **Short-term symptom is addressable** by the simpler
  per-instance latest-wins mutex pattern (separate todo)
  PLUS the open-url-specific Blocking-mode UX trade-off
  (open-url's row lags the user's current query by up to 2s,
  but other plugins render normally — assuming the
  investigation todo confirms incremental streaming).
- **Whether we need cancellation at all** depends on whether
  the per-keystroke URL-typing scenario is actually how
  users use the feature. Most URL entry happens by paste or
  by typing-then-pausing; the rapid-typing-multiple-URLs
  pattern is uncommon. Worth measuring before investing.

## When to revisit

- A second plugin lands that genuinely needs `lookup_blocking`
  in a hot path (i.e. on every keystroke). The per-instance
  fix doesn't help these plugins.
- User reports of open-url's row lagging visibly during fast
  typing become common.
- A new host import is proposed that's likely to take seconds
  (e.g. an LLM call, a translation API).

Until one of those triggers fires, the per-instance latest-
wins fix plus the existing UX should be sufficient.

## References

- `src-tauri/src/plugin_host.rs:449,560` — orchestrator
  search dispatch
- `src-tauri/src/wasm/runtime/instance.rs:41,232` — per-instance
  mutex
- `src-tauri/src/wasm/runtime/host/website_metadata.rs:72` —
  block_in_place call site
- `src-tauri/src/network/website_metadata/` — service that
  performs the actual fetch (would need cancelation
  refactor)
- `plugins/plugin-sdk/wit/torchsnap-plugin.wit:718-777` —
  WIT interface, would need a `Cancelled` variant
- `plugins/plugin-sdk/src/website_metadata.rs` — SDK wrapper,
  `Metadata` enum may grow a `Cancelled` arm or absorb it
- `src/launcher/hooks/useSearch.ts:95-102` — frontend
  generation check (would still discard, but cancellation
  prevents the wasted backend work)
- Sibling todo:
  `01kqf7h3r969zkc9g2qmk7fbsq-investigate-search-pipeline-blocking-scope.md`
  — investigates whether the empty-list symptom is per-row
  or pipeline-wide.
