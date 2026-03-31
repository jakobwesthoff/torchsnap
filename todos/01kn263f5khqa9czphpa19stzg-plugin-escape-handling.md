# Plugin ESC key handling for execute-activated views `[discuss]`

## Problem

When a plugin's custom UI is activated via `PostAction::ShowCustomUI`
(either through a global shortcut or an execute action on a catalog
entry), ESC does not behave correctly:

- **Query non-empty + ESC**: should clear query and stay in plugin
  view. Actually exits the plugin view entirely.
- **Query empty + ESC**: should dismiss the launcher. Actually exits
  the plugin view but leaves the launcher open in an empty state.

## Activation paths

The clipboard plugin (CatalogPlugin, not QueryPlugin) has two paths
that both result in `executePluginView` being set:

1. **Global shortcut** (`CmdOrCtrl+Shift+V`) → `handle_shortcut()`
   returns `PostAction::ShowCustomUI` → backend emits
   `activate-plugin-custom-ui` → frontend sets `executePluginView`
   and clears query.
2. **Execute action** (user selects "Clipboard History" catalog
   entry, presses Enter) → `execute()` returns
   `PostAction::ShowCustomUI` → frontend sets `executePluginView`
   and clears query.

Neither path involves prefix routing. `matchedPrefix` is always
`null` / `""` for these views.

## Root cause

`ClipboardView`'s `clipboard-escape` handler unconditionally calls
`goBack()` without checking query state.

`goBack()` (`handleGoBack` in Launcher.tsx) unconditionally:
1. Clears `executePluginView` → plugin view unmounts
2. Calls `setQuery("")` → no effect if already empty

When query is empty: `goBack()` exits the plugin view but never
calls `dismiss()`, leaving the launcher open in an empty state.
The launcher-level `launcher-escape` binding doesn't fire because
`clipboard-escape` at a higher layer already consumed the event.

When query is non-empty: `goBack()` exits the plugin view AND
clears the query in one step — the user expected only the query
to be cleared.

## Additional context

This was masked before the async search refactor. Previously
`useSearch` took one extra render cycle to clear `customPluginView`
on query change (async result had not arrived yet). The synchronous
reset in the refactored `useSearch` removed that buffer, making the
bug fully visible. The bug itself is pre-existing.

## Required behavior

Three ESC cases for execute-activated plugin views:

1. **Query non-empty** → clear query, stay in plugin view
2. **Query empty, activated via shortcut** → dismiss launcher
   (no prior launcher state to return to)
3. **Query empty, activated via execute** → go back to launcher
   with previous query and selection restored (the user was
   looking at search results and opened the plugin from there)

For prefix-activated views (not currently applicable to clipboard,
but relevant for future plugins):

1. **Query non-empty** → clear query, stay in plugin view
2. **Query empty** → go back to normal launcher (remove prefix)

## State snapshot problem

Case 3 requires restoring the launcher state from before the
plugin was opened. Currently both activation paths call
`setQuery("")` immediately, wiping the previous query. Even if
ESC correctly triggered `goBack()`, the user would return to an
empty launcher — not to the search results they were looking at.

This needs a snapshot/restore mechanism:
- On `PostAction::ShowCustomUI` from execute: save current query,
  selected index (and potentially scroll position) before clearing.
- On `goBack()`: restore the saved state.
- On shortcut activation: no snapshot needed (there is no prior
  state).

There is already an existing todo for this:
`01kmpdcmj1w94gtcnk8vwn8t4s-execute-triggered-custom-ui-state-snapshot.md`

## Design considerations (open — needs further discussion)

Several interrelated problems need solving together:

- Plugin needs a "clear query, stay in view" action — `goBack()`
  always exits, no alternative exists today.
- Plugin needs to know activation mode (shortcut vs execute) to
  choose between dismiss and restore on exit. Both paths currently
  set `executePluginView` identically.
- Execute-activated exit needs state snapshot/restore (separate
  todo: `01kmpdcmj1w94gtcnk8vwn8t4s`). Without it, "go back"
  returns to an empty launcher regardless.
- Prefix-activated views have different ESC semantics than
  execute-activated views — the solution must accommodate both.

No approach has been settled on yet. Initial options discussed:
(a) `clearQuery` prop, (b) conditional `goBack`, (c) smarter
`goBack` with internal query check. Each has trade-offs that need
further discussion, especially in relation to the state snapshot
mechanism.
