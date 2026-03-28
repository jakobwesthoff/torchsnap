# Frontend Memory Leak Cleanup

Fix JS-side memory leaks found during audit. These compound across
launcher hide/show cycles since the webview stays mounted.

Related: `01kmt2frxmdcycvkk7pnwqz90y-webview-memory-optimization.md` (broader
webview-level strategies), `docs/research/webview-garbage-collection.md`
(platform GC research).

## HIGH — Tauri Channel Leaks

### 1. `useControlChannel.ts` — Channel recreated without cleanup

`src/launcher/hooks/useControlChannel.ts`

The `useEffect` has `[resetState, setQuery]` as dependencies but no cleanup
return. Every time those change identity, a new `Channel` + `control_subscribe`
invoke is created. Old channels are never closed or dereferenced.

**Fix:** Use refs for `resetState`/`setQuery` so the effect runs exactly once
(empty deps). If re-subscription is genuinely needed, add a cleanup return that
notifies the backend to release the old channel.

### 2. `useSearch.ts` — Channel leaked per keystroke

`src/launcher/hooks/useSearch.ts`

Creates a new `Channel` on every `query` change with no cleanup. For a
10-character query, 10 channels are created and only the last is used. The
generation counter prevents stale state updates but doesn't release the
channel objects or their IPC registrations.

**Fix:** Add a cleanup return that no-ops the old channel's `onmessage`.
Consider debouncing channel creation or reusing a single channel across
queries.

## MEDIUM

### 3. `settingsStore.ts` — `listen()` unlisten dropped

`src/settingsStore.ts`

The `UnlistenFn` returned by `listen("settings-changed")` is discarded. Since
this is module-level singleton code it only registers once, but the unlisten is
permanently lost. Would double-register on hot reload.

**Fix:** Store the unlisten function. Call it if the module is ever torn down.

### 4. `usePluginStream.ts` — Abandoned channel keeps closure alive

`src/hooks/usePluginStream.ts`

When `sendPluginMessage` is called on re-subscribe, the old `Channel` is
abandoned with its `onmessage` closure still wired up. The `cancelled` flag
prevents state updates but the closure (holding `snapshotRef`, `listenersRef`)
stays alive until the backend stops sending.

**Fix:** Null out or no-op `channel.onmessage` in the cleanup return. Tauri
`Channel` has no explicit `.close()` API, so silencing the handler is the
practical path.

## Implementation Notes

- Issues 1 and 2 are the highest-impact fixes — they compound with every user
  interaction across hide/show cycles.
- All fixes are backwards-compatible and localized to their respective files.
- ClipboardView's `detailCacheRef` was initially flagged but is a false
  positive: the component unmounts on every dismiss (resetState clears
  query and executePluginView, collapsing the conditional render gate).
- After fixing, re-measure with `tools/bench-memory` to quantify the
  improvement.
