# Console source filter: unchecking a source while "all" selected inverts the selection

**Kind:** possible-bug
**Severity:** low

**Area:** src/devtools/console/useLogFilters.ts

## Problem
When the source filter is in the `"all"` state, the dropdown renders
every source with a *checked* checkbox
(`src/devtools/console/ConsoleToolbar.tsx:166-168`):

```ts
const checked =
  isAllSources || (filters.sources !== "all" && filters.sources.has(source));
```

Clicking such a checked entry calls `onToggleSource(source)`, and
`toggleSource` (`src/devtools/console/useLogFilters.ts:63-77`) does:

```ts
setSources((prev) => {
  if (prev === "all") {
    return new Set([source]);
  }
  ...
```

So from the "everything checked" state, clicking one checkbox to
*uncheck* it instead makes that source the *only* selected one — the
clicked box stays checked and all the others uncheck. The checkbox
affordance promises toggle semantics; the code implements
solo-on-first-click semantics.

## Impact
With several gadgets logging, a user who wants to hide one noisy
source (uncheck it from "all") gets the exact opposite: only that
source remains visible. Recovering requires re-checking every other
source or clicking "All sources".

## Suggested fix
Decide which semantics are intended:
- Toggle semantics (matches the checkbox UI): when `prev === "all"`,
  return `new Set(knownSources.filter(s => s !== source))` — this
  requires passing the known-source list into the hook or handling it
  in the toolbar.
- Solo semantics (click-to-solo, like some consoles): keep the logic
  but change the UI so unselected-state items don't render as checked
  checkboxes (e.g. highlight style instead).
