# Tree view silently drops spanEnd items whose spanStart is missing

**Kind:** possible-bug
**Severity:** low

**Area:** src/devtools/console/useTreeView.ts

## Problem
`buildTree` only records a `spanEnd` onto an existing node and
otherwise discards the item entirely
(`src/devtools/console/useTreeView.ts:81-86`):

```ts
} else if (kind.type === "spanEnd") {
  const node = spanNodes.get(kind.spanId);
  if (node) {
    node.spanEnd = item;
    node.inProgress = false;
  }
}
```

There is no `else` branch: a `spanEnd` whose `spanStart` is not in
the current item window is not attached anywhere, so it produces no
`FlatRow` and is invisible in tree mode. Orphan `spanEnd` items are a
real state, not just a theoretical one:

- The ring buffer holds 10k items
  (`DEFAULT_RING_BUFFER_CAPACITY`, `src-tauri/src/wasm/logging/mod.rs:55`);
  once eviction starts, `spanStart` items are evicted before their
  matching `spanEnd`.
- The history/subscribe gap in `useLogStream`
  (see `frontend/01kwg3kfadnzprpk0bq0xa8055-logstream-subscribe-gap-loses-items.md`)
  can swallow a `spanStart` while the `spanEnd` arrives later.

By contrast, orphan *messages* and orphan child spans fall back to
root-level rows (`useTreeView.ts:76-80, 89-94`), and the flat view
shows orphan `spanEnd` items normally. Only the tree view loses them.

## Impact
In tree mode after buffer eviction (long sessions) the completion
records of long-running spans — including their duration, the datum
the tree view exists to show — disappear without a trace, while the
flat view still shows them. Counts in the toolbar (`spanCount` counts
`spanEnd` items, `useLogFilters.ts:111-112`) then disagree with what
the tree displays.

## Suggested fix
In the `spanEnd`-without-node case, push the item as a root-level
`{ kind: "item", item }` child (same fallback as orphan messages) so
it renders via `LogItemRow`, which already handles `spanEnd` rows
with a duration badge.
