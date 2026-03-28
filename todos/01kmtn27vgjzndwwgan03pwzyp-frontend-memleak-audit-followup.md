# Frontend Memory Leak Audit Follow-up (2026-03-28)

Findings from a second audit of all frontend source files. The four issues
from `01kmtfq0erkn5vxqnzka4jz2ce-frontend-memory-leak-cleanup.md` are
excluded — three of those have been fixed, and the fourth
(`usePluginStream`) is revisited here with additional context.

## MEDIUM — `usePluginStream` channel cannot be silenced (design gap)

`src/hooks/usePluginStream.ts`, `src/lib/pluginMessage.ts`

The existing todo recommends nulling `channel.onmessage` in the cleanup
return. This is not feasible with the current API: `sendPluginMessage`
creates the `Channel` internally (line 26 of `pluginMessage.ts`) and
returns only the invoke promise. The caller has no reference to the
channel object and therefore cannot silence it.

The `cancelled` flag prevents state updates but the closure (holding
`snapshotRef`, `listenersRef`) stays alive until the backend stops
sending on the channel.

**Fix options:**

1. Return the channel alongside the promise from `sendPluginMessage`
   (e.g. `{ promise, channel }`), so the caller can null `onmessage`
   in cleanup.
2. Accept an `AbortSignal` parameter; when aborted, `sendPluginMessage`
   nulls the channel's `onmessage` internally.
3. Expose a dedicated `subscribePluginStream` helper that returns a
   handle with an explicit `close()` / `silence()` method.

Option 2 is the most ergonomic for React effects (`AbortController` in
setup, `controller.abort()` in cleanup).

## LOW — `useWindowLifecycle` async unlisten race

`src/launcher/hooks/useWindowLifecycle.ts:40-55`

The effect depends on `[dismiss, inputRef, mouseActiveRef]`. Cleanup
calls `.then()` on the listen promises to unlisten, but the next
effect invocation calls `appWindow.listen()` synchronously — before
the previous unlisten promises resolve. During that async gap both
old and new listeners are active simultaneously.

Currently safe because `dismiss` is stable (its `useCallback` depends
only on `resetState`, which itself depends only on state setters). But
if `resetState` ever gains an unstable dependency, duplicate focus/blur
handlers silently accumulate across re-runs.

**Fix:** Convert to the `await`-first pattern — `listen` returns a
promise, so the handler and unlisten can be stored in refs:

```ts
const unlistenRef = useRef<(() => void)[]>([]);

useEffect(() => {
  let stale = false;

  (async () => {
    const f = await appWindow.listen("tauri://focus", () => { ... });
    const b = await appWindow.listen("tauri://blur", () => { ... });
    if (stale) { f(); b(); return; }
    unlistenRef.current = [f, b];
  })();

  return () => {
    stale = true;
    for (const fn of unlistenRef.current) fn();
    unlistenRef.current = [];
  };
}, []);
```

Using refs for `dismiss` (same pattern as `useControlChannel`) would
let the deps array be empty, eliminating re-runs entirely.

## LOW — `useKeyBindings` effect runs every render (no dep array)

`src/keybindings/useKeyBindings.ts:50-88`

The first `useEffect` has no dependency array, so it executes after
every render. It clears and repopulates `handlersRef` and runs the
`hasChanged` structural comparison each time. When the comparison
returns true (e.g. `enabled` toggled, action list changed), it calls
`cleanupRef.current()` and `register()` to re-register.

This is intentional — the handler-ref indirection keeps registration
stable while handlers change. But the missing dep array means the
comparison and ref update run on every render of every component that
uses `useKeyBindings`, regardless of whether anything changed. Not a
memory leak per se, but unnecessary per-render work that scales with
the number of active keybinding consumers.

**Fix:** Move the handler-ref update out of the effect (it's a
render-time assignment, not a side effect). Then add a dependency
array that triggers re-registration only when the definitions
reference changes.

## NEGLIGIBLE — `Launcher.tsx` `activate-plugin-custom-ui` StrictMode double-mount

`src/launcher/Launcher.tsx:117-130`

Same async unlisten pattern as `useWindowLifecycle`. The effect has
`[]` deps so it runs once in production — no leak. Under React
StrictMode (dev only), effects mount/unmount/remount synchronously.
The async gap between `listen()` returning a promise and the cleanup
`.then()` resolving means both the old and new listeners can be
briefly active at the same time during the StrictMode double-mount
cycle. This can cause duplicate `activate-plugin-custom-ui` handling
in development.

Not a production concern. Mentioning for completeness; fix alongside
the `useWindowLifecycle` pattern if that gets addressed.
