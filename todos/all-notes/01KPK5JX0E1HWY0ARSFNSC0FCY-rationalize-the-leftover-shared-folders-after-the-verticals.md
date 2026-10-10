---
title: "Rationalize the leftover shared folders after the verticals"
kind: refactor
component: frontend
status: blocked
---
# Rationalize the leftover shared folders after the verticals

The mascot vertical is done (`src/mascot/`, `e469a5ab`). After the logger and gadget-runtime verticals are extracted, the horizontal `components/`, `hooks/`, `lib/` and `contexts/` folders will contain only genuinely shared, cross-feature code. This todo tracks the final rationalization of those leftovers. Low priority. Do it last, once the feature verticals are done, so the remaining contents are stable and the right groupings are clear.

## Expected remaining contents (post-verticals)

### `src/components/` (true design-system primitives)
- `Icon.tsx`
- `KeyBindingPill.tsx`, `KeyCap.tsx`, `ShortcutRecorder.tsx`
- `Slider.tsx`, `Switch.tsx`
- `ThemeToggle.tsx`
- `TitleBar.tsx`

### `src/hooks/` (shared cross-feature hooks)
- `useEmacsBindings.ts`
- `useHalfPageScroll.ts`
- `useResizeObserver.ts`
- `useSetting.ts`
- `useTheme.ts`

### `src/contexts/` (remaining shared contexts)
- `ThemeContext.tsx`, `ThemeProvider.tsx`
- `useLauncher.ts` (if not launcher-specific)
- `index.ts` barrel

### `src/lib/` (pure stateless utilities)
- `binarySearch.ts`, `sortedMerge.ts`
- `cn.ts`
- `command.ts` (Tauri command wrapper)
- `highlightText.tsx`
- `LruCache.ts`

## Options for consolidation

**Option A: keep the four folders as they are.** After the verticals, their remaining contents are small and genuinely shared. The horizontal folders are not a problem at this scale.

**Option B: merge into `src/ui/` and `src/lib/`.** Move components, hooks and contexts into `src/ui/`, and keep `src/lib/` for stateless utilities. This creates a clearer split between the design system and pure utilities.

**Option C: introduce a `src/theme/` sub-vertical.** Move `ThemeContext`, `ThemeProvider`, `ThemeToggle` and `useTheme` into `src/theme/`, parallel to `src/logger/`. This is consistent with the vertical approach.

Recommendation: decide between A and C at implementation time, based on how much theme code has grown by then.

## Relations

- Depends on: [Move the logger code into a src/logger vertical](01KPK5JX0E1HWY0ARSFNSC0FCV-move-the-logger-code-into-a-srclogger-vertical.md)
- Depends on: [Move the gadget runtime code into a src/gadget-runtime vertical](01KPK5JX0E1HWY0ARSFNSC0FCW-move-the-gadget-runtime-code-into-a-srcgadget-runtime-vertical.md)
