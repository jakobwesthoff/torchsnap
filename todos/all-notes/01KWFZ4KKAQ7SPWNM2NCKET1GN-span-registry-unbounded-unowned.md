---
kind: bug
severity: medium
status: open
area: [src-tauri/src/wasm/logging/spans.rs]
tags: [unconfirmed]
---

# Span registry: unbounded open spans, no ownership on parent/end

## Problem
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
   without bound. `MAX_SPAN_NESTING` caps depth, not count —
   root spans are unlimited. There is no per-gadget quota and
   no cleanup on gadget disable, so leaked spans survive the
   gadget's own lifetime.
2. **`span-end` has no ownership check.** `end(span_id, ...)`
   removes whatever id it is given (`:145-150`). The guest
   supplies an arbitrary `u64`, and ids are a global
   `fetch_add` counter, so a gadget can guess/iterate ids and
   close the host's or another gadget's open spans. The closed
   span's `CompletedSpan` is then emitted under the *victim's*
   source with bogus duration, corrupting devtools timelines.
3. **`parent` is similarly unchecked** (`:104-118`): a gadget
   can parent its spans under any foreign open span, splicing
   its entries into another source's tree in the devtools
   console (`useTreeView` groups by span ids).

## Impact
Unbounded host memory growth driven by guest behavior, plus
cross-gadget log-tree corruption/spoofing. Not exploitable
beyond diagnostics integrity, but diagnostics are exactly what
you rely on when a gadget misbehaves.

## Suggested fix
Track the owning `LogSource` per span (already stored) and
enforce it: `end` and parent-lookup only match spans whose
source equals the caller's; add a per-source open-span cap
(e.g. 1000, dropping oldest with a warn log) and drain a
gadget's open spans on disable.
