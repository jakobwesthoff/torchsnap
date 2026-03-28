# Frontend Memory Leak Cleanup

Fix JS-side memory leaks found during audit. These compound across
launcher hide/show cycles since the webview stays mounted.

Related: `01kmt2frxmdcycvkk7pnwqz90y-webview-memory-optimization.md` (broader
webview-level strategies), `docs/research/webview-garbage-collection.md`
(platform GC research).

## FIXED

### 1. `useControlChannel.ts` — Channel recreated without cleanup (FIXED)

Used refs for `resetState`/`setQuery` so the effect runs exactly once (empty
deps). Added cleanup return that no-ops `onmessage`. Note: Tauri automatically
cleans up the JS-side IPC registration when the Rust `Channel` is dropped
(sends `{ end: true }` marker), so this is mostly a safety net.

### 2. `useSearch.ts` — Channel leaked per keystroke (FIXED)

Added cleanup return that no-ops `onmessage` on query change. The Rust
`search_query` command is synchronous and drops the channel on return, which
triggers Tauri's automatic `{ end: true }` cleanup on the JS side. The no-op
is a belt-and-suspenders measure.

### 3. `settingsStore.ts` — `listen()` unlisten dropped (FIXED)

Stored the `UnlistenFn` and added `import.meta.hot.dispose()` to tear it down
during Vite HMR, preventing double-registration on module re-evaluation.

### 4. `usePluginStream.ts` — Abandoned channel keeps closure alive (FIXED previously)

Was already rewritten with `useSyncExternalStore` and a proper `cancelled`
cleanup return before this audit.

## REMAINING — Clipboard Subscribe Architecture

Tracked separately in
`01kmtn6msqqjpnc9aeb3rh8gnw-clipboard-subscribe-channel-architecture.md`.
Per-keystroke channel accumulation on the Rust side due to conflated
subscribe + query concerns.
