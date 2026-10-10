---
title: "Bound and own the span registry's open spans"
kind: bug
component: gadget-host
---
# Bound and own the span registry's open spans

The span registry keeps every open span in one process-wide map shared by the host and all gadgets. A gadget controls when its spans start and end, so it can grow the map without limit and can close or parent spans that belong to the host or to another gadget.

## Problem
Read in the code, not reproduced.

`SpanRegistry` keeps open spans in a process-wide
`Mutex<HashMap<u64, OpenSpan>>`
(`src-tauri/src/wasm/logging/spans.rs:87`) shared by the host
and every gadget. Three gaps, all reachable from the guest via
the `logging::span-start` / `span-end` host imports
(`runtime/host/logging.rs:56-105`):

1. **Never-ended spans leak forever.** `start` inserts
   (`spans.rs:131-134`); the only removal is `end`
   (`:145-150`). A gadget that calls `span-start` without a
   matching `span-end` (bug or hostile loop) grows the map
   without bound. `MAX_SPAN_NESTING` caps depth, not count.
   Root spans are unlimited. There is no per-gadget quota and
   no cleanup on gadget disable, so leaked spans survive the
   gadget's own lifetime.
2. **`span-end` has no ownership check.** `end(span_id, ...)`
   removes whatever id it is given (`:145-150`). The guest
   supplies an arbitrary `u64`, and ids are a global
   `fetch_add` counter, so a gadget can guess or iterate ids and
   close the host's or another gadget's open spans. The closed
   span's `CompletedSpan` is then emitted under the *victim's*
   source with bogus duration, corrupting devtools timelines.
3. **`parent` is similarly unchecked** (`:104-118`): a gadget
   can parent its spans under any foreign open span, splicing
   its entries into another source's tree in the devtools
   console (`useTreeView` groups by span ids).

## Impact
Unbounded host memory growth driven by guest behavior, plus
cross-gadget log-tree corruption and spoofing. Not exploitable
beyond diagnostics integrity, but diagnostics are exactly what
you rely on when a gadget misbehaves.

## Suggested fix
Track the owning `LogSource` per span (already stored) and
enforce it: `end` and parent-lookup only match spans whose
source equals the caller's. Add a per-source open-span cap
(e.g. 1000, dropping oldest with a warn log) and drain a
gadget's open spans on disable.
