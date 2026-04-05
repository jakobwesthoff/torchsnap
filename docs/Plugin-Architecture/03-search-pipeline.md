# Search Pipeline

## Entry Point

The Tauri command `search` (`search/mod.rs`) receives a query string and a
`Channel<SearchMessage>`, then delegates to `PluginHost::search()`.

## Prefix Routing Path

`plugin_host.rs:451-497`

If `find_prefix_match()` identifies a query plugin whose registered prefix
starts the query:

1. Strips the prefix from the query
2. Spawns one `spawn_blocking` task for that plugin's `search()`
3. Collects at most one `PluginResponse`
4. `CustomUI` responses are allowed in this path
5. Sends a single `SearchResults` message, then `Done`

Only one plugin can win prefix routing (longest prefix match wins).

## Normal (Non-Prefix) Path

`plugin_host.rs:499-618`

Two phases run sequentially:

### Phase 1: Catalog Search

Runs synchronously via `spawn_blocking` → `search_catalogs_static()`:

1. Collects `entries()` from all enabled catalog-mode plugins
2. Uses nucleo (`Matcher`, `Pattern::indices`) to fuzzy-score each entry's
   title
3. Falls back to scoring `title + keywords` if the title alone doesn't match
4. Builds `ScoredEntry` with `title_positions`
5. Applies frecency bonuses
6. Sorts by `(score DESC, source ASC, id ASC)`
7. Sends immediately as a `SearchResults` message

### Phase 2: Query Plugin Fan-Out

1. Each enabled query-mode plugin gets its own `mpsc::channel<PluginResponse>(4)`
   and a `spawn_blocking` task
2. The host polls all receivers in a spin-yield loop:
   - `try_recv()` on each receiver
   - If a response arrives: process it, apply frecency, sort, send as
     `SearchResults`
   - If a receiver disconnects: remove it via `swap_remove`
   - If nothing ready: `tokio::task::yield_now().await`
3. Loop ends when all receivers are drained

### `process_plugin_response()`

`plugin_host.rs:624-663`

Converts a `PluginResponse` into `(Option<(ViewKind, PluginViewRef)>,
Vec<ScoredEntry>)`:

- `Results` → entries only, no view ref
- `CustomUI` → view ref + entries, but **only in prefix mode**. In non-prefix
  mode, `CustomUI` is silently downgraded to plain results with no warning
- `InlineUI` → view ref + entries (allowed in both modes)

### Inline View Slot Contention

An `inline_claimed` flag tracks whether any plugin has already claimed the
inline slot in a given search. If a second plugin requests `InlineUI`, its
view ref is dropped and an `eprintln!` warning is emitted. The winner is
whichever plugin's channel message arrives first — this is non-deterministic.

## Sort Comparator

The three-key sort `(score DESC, source ASC, id ASC)` is implemented three
times:

1. **Rust, catalog results** (`plugin_host.rs:749-754`)
2. **Rust, query plugin results** (`plugin_host.rs:571-575`)
3. **TypeScript, `useSearch.ts`** (`compareEntries` function)
4. **TypeScript, `Launcher.tsx`** (binary search comparator for selection
   stability)

The frontend relies on results arriving pre-sorted from the backend (implicit
contract, not type-enforced) and uses `sortedMerge()` with the same comparator
to merge incremental batches.

## Frecency

Applied in `apply_frecency_bonuses()` (`plugin_host.rs`). Each plugin has a
scoped `PluginFrecency` handle. Frecency is recorded before execution (in
`execute()`), so intent is captured even if the action fails.
