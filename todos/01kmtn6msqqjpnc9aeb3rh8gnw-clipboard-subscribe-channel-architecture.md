# Clipboard Subscribe Channel Architecture

The clipboard plugin's subscribe mechanism conflates two concerns into a
single IPC call, causing per-keystroke channel accumulation on the Rust
side.

## Problem

`ClipboardView` calls `usePluginStream("subscribe", { query })` where
`query` changes on every keystroke. Each call:

1. Creates a new Tauri `Channel` and sends it to the backend
2. The backend appends it to an unbounded `Vec<Channel>` (`SharedState::subscribers`)
3. Returns a query-filtered snapshot as the initial response

But `notify_subscribers` always broadcasts the **unfiltered** history
(`search_history(None)`) — the query parameter is only used for the
initial response. So the per-keystroke re-subscribe is:

- Useful for getting filtered snapshots (return value)
- Wasteful for push notifications (accumulates redundant channels)

Stale channels are only cleaned up lazily via `retain` on the next
clipboard event. Between events, O(keystrokes × show/hide cycles)
channels accumulate.

## Proposed Fix

Separate the two concerns:

1. **Subscribe once on mount** — a single long-lived channel for push
   notifications. No query parameter needed since notifications are
   always unfiltered. Backend stores this as a single `Option<Channel>`
   (or replaces on re-subscribe) rather than appending to a `Vec`.

2. **Query-filtered snapshots as a regular one-shot call** — a separate
   `"search"` or `"list"` method that takes `{ query }` and returns
   filtered results without touching the subscriber channel. Called on
   every keystroke via a normal `invoke`, not `usePluginStream`.

This cleanly separates "I want live updates" from "I want filtered
results" and eliminates channel accumulation entirely.

## Alternatives Considered

- **Replace-instead-of-append**: Simpler backend-only fix (clear `Vec`
  before push, or use `Option<Channel>`). Stops accumulation but
  doesn't fix the conceptual conflation. Per-keystroke subscribe still
  creates/drops channels unnecessarily.

- **Backend query-aware notifications**: Store the query per subscriber
  and filter in `notify_subscribers`. Adds complexity and couples
  notification to frontend state.

## Related

- `todos/01kmtfq0erkn5vxqnzka4jz2ce-frontend-memory-leak-cleanup.md` —
  broader memory leak audit (issues 1 & 2 fixed, issue 3 fixed, issue 4
  was already fixed)
- `src/plugins/clipboard/ClipboardView.tsx` — frontend subscribe call
- `src-tauri/src/plugins/clipboard/mod.rs` — backend `handle_message`
- `src-tauri/src/plugins/clipboard/storage.rs` — `SharedState::subscribers`
