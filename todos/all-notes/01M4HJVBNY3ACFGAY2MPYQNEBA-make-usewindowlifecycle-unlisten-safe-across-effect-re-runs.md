---
title: "Make useWindowLifecycle unlisten safe across effect re-runs"
kind: refactor
component: frontend
impact: low
origin: review
tags: [memory, concurrency]
---
# Make useWindowLifecycle unlisten safe across effect re-runs

Found in the second frontend memory leak audit (2026-03-28) of all frontend
source files. Severity in the audit: low. The unlisten of the focus and blur
listeners in `useWindowLifecycle` races with the next effect run. Nothing leaks
today, but a future change to a dependency would make it leak silently.

## Problem

`src/launcher/hooks/useWindowLifecycle.ts`, around lines 40-55

The effect depends on `[dismiss, inputRef, mouseActiveRef]`. Cleanup calls
`.then()` on the listen promises to unlisten, but the next effect invocation
calls `appWindow.listen()` synchronously, before the previous unlisten promises
resolve. During that async gap both old and new listeners are active
simultaneously.

Currently safe because `dismiss` is stable (its `useCallback` depends only on
`resetState`, which itself depends only on state setters). But if `resetState`
ever gains an unstable dependency, duplicate focus/blur handlers silently
accumulate across re-runs.

## Suggested fix

Convert to the `await`-first pattern. `listen` returns a promise, so the
handler and unlisten can be stored in refs:

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

Using refs for `dismiss` (same pattern as `useControlChannel`) would let the deps
array be empty, eliminating re-runs entirely.
