# Search Pipeline

End-to-end flow of a single keystroke from the launcher input through the
host, into plugins, and back up to the React UI as streaming batches.

## Frontend entry: `useSearch`

`src/launcher/hooks/useSearch.ts:94-158`

Each query change bumps a monotonic `generationRef` counter, allocates a
fresh per-source accumulator `Map`, and constructs a Tauri
`Channel<SearchMessage>`. The hook then issues the Tauri command
`search` with `{ query, onResults: channel }`.

The channel's `onmessage` handler is gated by the generation counter:
messages from a superseded query are dropped on arrival
(`useSearch.ts:102`). On `searchResults` it replaces the per-source
slice in the accumulator, flattens all slices, and re-sorts using
`compareEntries` (which mirrors the Rust `cmp_sort_key`). On `done` it
flips loading off.

`customPluginView`, `inlinePluginView`, and `matchedPrefix` use a
"first non-null wins per generation" capture so the catalog batch
(which carries `null` for all three) cannot clobber a later
inline/custom view ref.

## Tauri command: `search`

`src-tauri/src/search/mod.rs:26-34`

Async command that delegates straight to
`PluginHost::search(&query, &on_results)`. The channel is owned by the
frontend; the host pushes `SearchMessage` values into it via
`on_results.send(...)` and the channel serializes them across the
Tauri bridge.

Message shape: `src-tauri/src/search/types.rs:275-304`. Variants are
`SearchResults { source, entries, customPluginView, inlinePluginView,
matchedPrefix }` and `Done`. `source` is `ResultSource::Catalog` or
`ResultSource::Plugin { id }`.

Sibling commands in the same module:
- `search_execute` (`mod.rs:47-62`) — wraps `PluginHost::execute` in
  `tokio::task::spawn_blocking`. The blocking pool is mandatory because
  plugin `execute()` may reach `http::fetch`, which calls
  `Handle::current()` inside reqwest. Tauri's sync IPC thread has no
  Tokio context.
- `plugin_message` (`mod.rs:73-93`) — same `spawn_blocking` rationale
  for `handle-message` RPC.

## Host dispatch: `PluginHost::search`

`src-tauri/src/plugin_host.rs:449-610`

Two branches: prefix-exclusive and the normal catalog + fan-out path.
Empty queries short-circuit by sending only `Done`
(`plugin_host.rs:450-453`).

### Prefix-exclusive routing (ADR 0012)

`plugin_host.rs:460-511`, `find_prefix_match` at
`plugin_host.rs:935-955`.

`find_prefix_match` walks every active plugin slot, examining each
prefix declared by `Plugin::search_prefixes()`
(`plugins/mod.rs:242`). The longest prefix that is a literal `starts_with`
match wins; ties are not possible because length is the sole key and
disabled slots are skipped.

When a prefix matches:

1. The matched prefix is stripped from the query
   (`plugin_host.rs:462`).
2. The plugin's `search(stripped, Some(prefix))` runs on a single
   `tokio::task::spawn_blocking` (`plugin_host.rs:468-472`).
3. The returned `PluginResponse` is fed to `process_plugin_response`
   with `allow_custom_ui = true` (`plugin_host.rs:479-481`).
4. Frecency bonuses are applied to the entries
   (`plugin_host.rs:498`), then a stable score-DESC sort is used —
   not the full `cmp_sort_key` — so the plugin's intended ordering
   for equal-score items is preserved (`plugin_host.rs:499`).
5. One `SearchResults` message is emitted with `matched_prefix:
   Some(prefix)`, followed by `Done` (`plugin_host.rs:502-510`).

No catalog search runs. No other plugin runs. This is the "exclusive"
in ADR 0012's exclusive routing.

### Normal path: catalog + concurrent query plugins (ADR 0023)

When no prefix matches, every active plugin participates regardless of
whether it also declares prefixes — that is ADR 0023.

#### Phase 1: catalog (synchronous, single batch)

`plugin_host.rs:519-545`, `search_catalogs_static` at
`plugin_host.rs:633-706`.

The host clones an `Arc<dyn Plugin>` for every active slot and offloads
to `spawn_blocking`. Inside the blocking task:

- A nucleo `Matcher` with `Config::DEFAULT { prefer_prefix: true }` and
  a smart-case, smart-normalization `Pattern` are built once
  (`plugin_host.rs:642-645`).
- For each plugin, `entries()` is called and every `CatalogEntry` is
  scored title-first via `Pattern::indices`
  (`plugin_host.rs:660-662`). On a title miss, the plugin's `keywords`
  are appended to the title and re-scored with `Pattern::score` (no
  positions kept) (`plugin_host.rs:666-672`).
- Successful matches produce a `SourcedEntry` with `title_positions`
  derived from the grapheme indices, and an empty
  `subtitle_positions` (`plugin_host.rs:678-693`).
- `FrecencyStore::apply_scores` adds per-source bonuses to the slice
  this plugin contributed (`plugin_host.rs:697-698`).
- The full vector is sorted by `SourcedEntry::cmp_sort_key`: score
  DESC, source ASC, id ASC (`plugin_host.rs:703`,
  `search/types.rs:194-201`).

The host always emits exactly one `SearchResults { source: Catalog,
... }` message even when the vector is empty
(`plugin_host.rs:536-545`). The frontend uses the catalog batch as the
canonical "new generation, recompute" signal.

#### Phase 2: query-plugin fan-out

`plugin_host.rs:547-609`.

Every active slot is dispatched in parallel via a `tokio::task::JoinSet`
of `spawn_blocking` tasks (`plugin_host.rs:549-561`). Each task calls
`plugin.search(query, None)` (no `matched_prefix` because prefix routing
already exclusive-shorted) and yields `(source, response)`.

The host drains the `JoinSet` with `join_next().await`
(`plugin_host.rs:567-607`). Order of arrival is non-deterministic.
Per response:

- `None` → skip (catalog-only or query-but-not-relevant plugins;
  `plugin_host.rs:570-573`).
- `Some(response)` → `process_plugin_response(..., allow_custom_ui:
  false)`, frecency bonus, sort by `cmp_sort_key`, emit
  `SearchResults` (`plugin_host.rs:575-606`).

An empty entry list is still emitted — the per-source key on the
frontend uses the message to evict that source's prior entries
(`plugin_host.rs:598-599`).

After the `JoinSet` drains, the host emits `Done`
(`plugin_host.rs:609`).

### Concurrency model

- Catalog phase: one `spawn_blocking` task. Blocks Phase 2 from
  starting because the await is sequential.
- Query phase: N concurrent `spawn_blocking` tasks (one per active
  plugin) on Tokio's blocking pool. The host streams results back as
  each task completes — no all-or-nothing barrier.
- The blocking pool is mandatory for the same Tokio-runtime-context
  reason as `search_execute`: WASM plugins reach host imports like
  `http::fetch` and `command::run` from inside `search()`.

### Stale-call elision (WASM only)

`src-tauri/src/wasm/runtime/instance.rs:42-62, 263-284`.

WASM components are single-threaded by spec, so every guest export
serializes on the per-instance store mutex. When a guest blocks
inside a host import (e.g. `website-metadata::lookup` in `Blocking`
mode), every later keystroke's `search()` call queues behind it.
Without elision, FIFO would drain the entire backlog after the slow
call finishes — wasting compute on results the frontend would
discard by generation anyway.

`WasmPluginInstance::search` increments
`search_generation: AtomicU64` on entry, then re-checks the counter
*after* acquiring the store lock. If a newer call has registered
during the wait, the older call returns
`PluginResponse::Results(vec![])` without invoking the guest
(`instance.rs:268-275`).

Native plugins do not have this problem and do not need this gate.

## `process_plugin_response`

`plugin_host.rs:878-930`.

Translates `PluginResponse` (host-internal, in
`search/types.rs:222-239`) into
`(Option<(ViewKind, PluginViewRef)>, Vec<SourcedEntry>)`:

- `Results(list)` → no view ref, entries wrapped with
  `SourcedEntry::new(source, ...)`.
- `CustomUI { view, data, results }` with `allow_custom_ui = true` →
  view ref + entries.
- `CustomUI { ... }` with `allow_custom_ui = false` → view ref dropped
  with an `eprintln!` warning, entries kept (ADR 0021: `CustomUI` is
  prefix-mode-only).
- `InlineUI { view, data, results }` → view ref + entries in either
  mode.

### Inline slot contention

In the fan-out path only one plugin can claim the inline slot per
search. The host tracks `inline_claimed: bool` across the
`join_next` loop (`plugin_host.rs:565, 582-596`). The first arriving
`InlineUI` wins; later inline view refs are dropped with a warning.
Because completion order is non-deterministic, the winner is
non-deterministic.

The prefix path is single-plugin so the contention question does not
arise there.

## Sort comparator

The key `(score DESC, source ASC, id ASC)` is implemented in:

- Rust catalog phase: `plugin_host.rs:703` (`cmp_sort_key`).
- Rust fan-out phase: `plugin_host.rs:580` (`cmp_sort_key`).
- Rust prefix phase uses score-DESC only, not the full key
  (`plugin_host.rs:499`); see the comment at
  `plugin_host.rs:492-497` for the reason.
- `SourcedEntry::cmp_sort_key`: `search/types.rs:194-201`.
- TypeScript: `src/launcher/compareEntries.ts`, used in
  `useSearch.ts:126` and in the launcher's selection-stability binary
  search (`src/launcher/Launcher.tsx`).

The frontend relies on each backend batch arriving pre-sorted. The
contract is implicit (no type enforces it).

## Frecency

`plugin_host.rs:498` (prefix), `:579` (fan-out), `:697-698` (catalog).

Frecency is a host service. Each plugin slot has a scoped
`PluginFrecency` handle. After a plugin's results are scored, the host
calls `FrecencyStore::apply_scores(source, &mut entries)` to add per-id
bonuses to that plugin's slice. Plugins do nothing.

Recording happens on the *execute* side: `PluginHost::execute` calls
`self.frecency.record(source, entry_id)` *before* dispatching to the
plugin (`plugin_host.rs:734`), so user intent is captured even when
the action errors.

The WIT `frecency` interface (`plugin-sdk/wit/torchsnap-plugin.wit:126-144`)
is purely read-side — it lets a plugin query its own top items to
drive UI like the emoji picker's empty-query browse mode. It is not
involved in the search-result-bonus path.

## Plugin contract

Native plugins implement `crate::plugins::Plugin`
(`plugins/mod.rs:112-273`). The search-relevant entry points:

- `id() -> &str` — the `source` field on every emitted entry.
- `search_prefixes() -> &[String]` — empty by default.
- `entries() -> Vec<CatalogEntry>` — empty by default (query-only
  plugins).
- `search(query, matched_prefix) -> Option<PluginResponse>` — `None`
  by default (catalog-only plugins). `matched_prefix.is_some()` iff
  this is the prefix-exclusive path.
- `execute(entry_id, action_id, app) -> Result<PostAction>`.

WASM plugins implement the WIT `search` interface
(`plugin-sdk/wit/torchsnap-plugin.wit:694-761`) and reach the host
through `WasmPluginBridge` (`wasm/bridge.rs:903-950`), which adapts
the WIT types to the native `Plugin` trait. The bridge's
`search_prefixes()` reads `manifest.plugin.prefixes` from
`manifest.toml` rather than a guest call, so prefix matching does not
touch the WASM store mutex.

WIT differences worth noting:

- WIT `search-response` has a `nothing` variant that the bridge maps
  to `Plugin::search` returning `None`. An empty
  `Results(vec![])` is distinct: it means "I am participating but
  have no entries", and is forwarded so per-source eviction works
  (`wasm/bridge.rs:936-940`).
- WIT `scored-entry` carries `title-highlight-positions` and
  `subtitle-highlight-positions` as UTF-16 offsets, which match the
  host's `Utf16Positions`.

## Sample plugin shapes

- `plugins/bangs/` — query plugin with a prefix (`!`). Demonstrates
  prefix-exclusive routing.
- `plugins/calculator/` — query plugin with a prefix (`=`).
- `plugins/emoji-picker/` — catalog plugin plus prefix browse mode
  with a `frecency::top-items` empty-query path.
- `plugins/hello-world/` — minimal catalog plugin, no `search()`.

## Custom vs inline UI vs structured results (ADR 0021)

Three response shapes flow through the same `SearchResults` message:

| Variant | Where allowed | Frontend effect |
|---|---|---|
| `Results` | always | merged into the result list |
| `InlineUI` | always (one-per-search winner) | mounts an inline component above the list; entries also merged |
| `CustomUI` | prefix mode only | replaces the list wholesale with the plugin's view |

`PluginViewRef` (`search/types.rs:251-256`) carries `pluginId`,
`view`, and optional JSON `data`. The frontend uses `pluginId` to
look up the React component in its plugin registry; `view` selects
between `views[view]` (custom) or `inlineViews[view]` (inline).

## Cancellation semantics

The Tauri command future is awaited on the host side; if the
frontend tears down (e.g. the launcher closes), the channel sender
errors are silently dropped (`let _ = on_results.send(...)`). Tasks
already running on the blocking pool are not cancelled — they run to
completion and their messages are then discarded by the frontend's
generation guard. The WASM stale-elision path is the only place that
actively avoids redundant work; everywhere else, "cancellation" means
"the result is discarded after the fact".
