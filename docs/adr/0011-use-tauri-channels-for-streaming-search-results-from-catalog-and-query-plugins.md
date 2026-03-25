# 11. Use Tauri channels for streaming search results from catalog and query plugins

Date: 2026-03-25

## Status

Accepted

## Context

Plugins fall into two fundamentally different categories:

- **Catalog plugins** (app launcher, system commands, contacts,
  emoji): Provide a finite, known list of entries upfront. The host
  can filter and rank them instantly using nucleo.
- **Query plugins** (file search, web search, calculator): Cannot
  provide a full list. They generate/filter results themselves based
  on the raw query. Response time varies — file search is fast, web
  search has network latency.

A single synchronous Tauri command that waits for all plugins would
block until the slowest plugin responds, destroying perceived speed.
Plain Tauri events would work but lack scoping (results from
concurrent queries could interleave), ordering guarantees, and typed
message contracts.

Alternatives considered:

- **Single blocking command**: Simple but unacceptable latency when
  any query plugin is slow.
- **Multiple parallel commands**: One per plugin. Frontend merges.
  Works but creates N concurrent IPC round-trips and requires the
  frontend to manage correlation/cancellation manually.
- **Global Tauri events with query IDs**: Unscoped, untyped,
  requires manual correlation of events to queries. Race-prone.
- **Tauri channels**: Typed, ordered, scoped to a single invocation,
  with natural cancellation semantics.

## Decision

### Two plugin modes

Each plugin declares its mode at registration:

- **Catalog mode**: Plugin provides its full entry list to the host.
  The host owns filtering via nucleo. The plugin never sees
  individual keystrokes. Updates to the catalog (e.g. new app
  installed) are pushed by the plugin when its data changes.
- **Query mode**: Plugin receives the raw query string and returns
  its own filtered, scored results. The host interleaves them with
  catalog results but does not re-filter them.

### Single Tauri command with a channel

One command handles all search:

```rust
#[tauri::command]
async fn search(query: String, channel: tauri::ipc::Channel<SearchMessage>)
```

The command sends results progressively through the channel using a
message enum:

```rust
enum SearchMessage {
    /// Instant results from all catalog plugins, filtered by nucleo.
    CatalogResults(Vec<ScoredEntry>),
    /// Streaming results from a single query plugin.
    PluginResults { plugin_id: String, entries: Vec<ScoredEntry> },
    /// All plugins have finished responding.
    Done,
}
```

### Cancellation via channel drop

When a new keystroke arrives, the frontend drops the previous
channel and opens a new one. The Rust side detects the closed
channel and cancels any in-flight query plugin work. This naturally
handles debouncing at the transport level — stale queries are
abandoned without explicit cancellation logic.

### Filtering lives in Rust

Catalog filtering uses nucleo on the Rust side. The frontend never
performs fuzzy matching — it receives pre-filtered, pre-scored
results with match highlight positions and renders them.

Query plugins perform their own filtering. The host passes through
their results with the scores they provide.

### Frontend rendering

The frontend's role is:

1. Call `search(query, channel)` on each keystroke (debounced)
2. Receive `CatalogResults` first (sub-millisecond) and render
3. Receive `PluginResults` incrementally, interleave into the
   displayed list by score, re-render
4. Receive `Done` to finalize loading state

## Consequences

- Catalog results appear instantly regardless of how slow query
  plugins are. The UI is never blocked waiting for network or disk.
- Tauri channels provide scoped, typed, ordered delivery. No global
  event bus pollution or manual query correlation.
- Channel drop gives free cancellation — outdated queries are
  abandoned automatically when the user types another character.
- Nucleo runs Rust-side where it's fastest and where catalog data
  already lives (WASM plugins are Rust). No serialization overhead
  for the matching step.
- The frontend is a pure renderer. It never owns filtering logic,
  which simplifies the React layer and eliminates JS-side matching
  libraries.
- Query plugins must return scored results in a compatible format
  so the host can interleave them with catalog results. This
  requires a shared scoring contract.
- The two-mode distinction (catalog vs query) must be part of the
  plugin manifest. A plugin cannot be both — though a plugin could
  internally register separate catalog and query providers if needed.
