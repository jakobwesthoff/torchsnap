# Final cleanup: shared `ui/` primitives after vertical extraction

After completing the mascot, logger, and plugin-runtime verticals, the
horizontal `components/`, `hooks/`, `lib/`, and `contexts/` folders will
contain only genuinely shared, cross-feature code. This todo tracks the
final rationalization of those leftovers.

Do this LAST — after the three feature verticals are done — so the
remaining contents are stable and the right groupings are clear.

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

**Option A — keep the four folders as-is**: After the verticals, their
remaining contents are small and genuinely shared. The horizontal folders
aren't a problem at this scale.

**Option B — merge into `src/ui/` + `src/lib/`**: Move components, hooks,
and contexts into `src/ui/`; keep `src/lib/` for stateless utilities. Creates
a clearer "design system" vs "pure utility" split.

**Option C — introduce `src/theme/` sub-vertical**: Move `ThemeContext`,
`ThemeProvider`, `ThemeToggle`, and `useTheme` into a `src/theme/` vertical,
parallel to `src/logger/`. Consistent with the vertical approach.

Recommendation: decide Option A vs C at implementation time based on how
much theme code has grown by then.

## Priority: LOW (do last, depends on the three feature verticals being done)
