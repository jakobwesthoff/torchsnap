# Clipboard: Lazy detail loading + virtual scroll

**Priority: next**

The subscribe/notify flow currently sends full `ClipboardHistoryEntry`
objects (with all format arrays and resolved image paths) for every
entry. This doesn't scale and transmits data the user never sees.

## Design

### List query (subscribe/notify)

Return only what the list row needs:
- `id`
- `primary_format` — determines the icon (text, image, files)
- `preview` — truncated to ~100 chars
- `captured_at`

No format arrays, no image paths, no limit on entry count. The
frontend renders this in a windowed virtual list (same pattern as the
primary result list using something like `useWindowedGrid`).

### Detail query (`get_entry` message)

New `handle_message` method that returns the full entry for a single
ID on selection:
- All format types
- Full text content (or more of it)
- Resolved image path for `convertFileSrc`

Called when the user navigates to an entry. The frontend caches the
last N detail results so rapid arrow-key navigation doesn't re-fetch
already-seen entries.

### Live updates

`notify_subscribers` sends the lightweight list-level data only. When
a new entry arrives and the user has it selected, the frontend
triggers a detail fetch.

## Changes needed

### Backend
- New `ClipboardListEntry` type (id, primary_format, preview, captured_at)
- `query_history` returns `Vec<ClipboardListEntry>` instead of full entries
- New `get_entry` handler in `handle_message`
- `notify_subscribers` sends lightweight list

### Frontend
- `ClipboardView` uses `ClipboardListEntry` for the list
- Selection triggers `sendMessage("get_entry", { id })` for the preview
- Cache recent detail results (e.g., LRU of 10-20 entries)
- Virtual scrolling for the entry list (no fixed height limit)
