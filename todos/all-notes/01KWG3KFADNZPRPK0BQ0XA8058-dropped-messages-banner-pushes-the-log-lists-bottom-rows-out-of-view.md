---
title: "Dropped-messages banner pushes the log list's bottom rows out of view"
kind: bug
component: frontend
impact: low
---
# Dropped-messages banner pushes the log list's bottom rows out of view

When the dropped-messages banner appears, the log list's scroll container
extends past the window bottom. The newest rows are clipped even while
auto-scroll treats them as pinned to the bottom.

## Problem
Read in the code.

Both list components render the dropped-messages banner as a normal-flow
sibling *above* a scroll container that is still sized to the full parent
height. `src/devtools/console/LogList.tsx`, around lines 116-126:

```tsx
<div className="relative h-full">
  {/* Dropped messages banner */}
  {droppedCount > 0 && (
    <div className="flex items-center justify-center gap-2 py-1.5 ...">
      ...
    </div>
  )}

  {/* Scrollable log list */}
  <div ref={parentRef} className="h-full overflow-y-auto" onScroll={handleScroll}>
```

The identical structure exists in `src/devtools/console/TreeLogList.tsx`, around
lines 234-244. The root is `h-full` inside ConsoleTab's `flex-1 min-h-0` slot
(`src/devtools/console/ConsoleTab.tsx`, around line 88). When the banner appears,
banner height plus the `h-full` scroll viewport exceeds the slot, so the scroll
container's bottom edge (roughly the banner's height, ~28px) extends past the
window bottom. Auto-scroll aligns the newest row to the *scroll element's*
bottom (`virtualizer.scrollToIndex(..., { align: "end" })`, `LogList.tsx`, around
line 92), which is exactly the clipped region.

The `isAtBottom` math (`el.scrollHeight - el.scrollTop - el.clientHeight <
AUTO_SCROLL_THRESHOLD`, `LogList.tsx`, around line 79) still works because it is
internal to the scroll element. What the user sees as "the bottom" is the newest
rows partially or fully hidden. The floating "scroll to bottom" button
(`absolute bottom-4`) also ends up positioned relative to the oversized root.

## Impact
Precisely when the console is under high volume (the only time
`droppedCount > 0`), the newest log rows are cut off at the window edge even
though auto-scroll believes it is pinned to the bottom.

## Suggested fix
Make the root a flex column and let the scroll container flex:
`<div className="relative h-full flex flex-col">` with the scroll div as
`flex-1 min-h-0 overflow-y-auto` (in both `LogList.tsx` and `TreeLogList.tsx`).
