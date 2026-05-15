# Search Pipeline

End-to-end flow of a single keystroke from the launcher input through the
host, into gadgets, and back up to the React UI as streaming batches.

## Frontend entry: `useSearch`

`src/launcher/hooks/useSearch.ts`

Each query change bumps a monotonic `generationRef` counter, allocates a
fresh per-source accumulator `Map`, and constructs a Tauri
`Channel<SearchMessage>`. The hook then issues the Tauri command
`search` with `{ query, onResults: channel }`.

The channel's `onmessage` handler is gated by the generation counter:
messages from a superseded query are dropped on arrival. On
`searchResults` it replaces the per-source slice in the accumulator,
flattens all slices, and re-sorts using `compareEntries` (which mirrors
the Rust `cmp_sort_key`). On `done` it flips loading off.

`customGadgetView`, `inlineGadgetView`, and `matchedPrefix` use a
"first non-null wins per generation" capture so the catalog batch
(which carries `null` for all three) cannot clobber a later
inline/custom view ref.

## Tauri command: `search`

`src-tauri/src/commands/mod.rs`

Async command that delegates straight to
`GadgetHost::search(&query, &on_results)`. The channel is owned by the
frontend; the host pushes `SearchMessage` values into it via
`on_results.send(...)` and the channel serializes them across the
Tauri bridge.

Message shape: `src-tauri/src/commands/types.rs`. Variants are
`SearchResults { source, entries, customGadgetView, inlineGadgetView,
matchedPrefix }` and `Done`. `source` is `ResultSource::Catalog` or
`ResultSource::Gadget { id }`.

Sibling commands in the same module:
- `search_execute`: wraps `GadgetHost::execute` in
  `tokio::task::spawn_blocking`. The blocking pool is mandatory because
  gadget `execute()` may reach `http::fetch`, which calls
  `Handle::current()` inside reqwest. Tauri's sync IPC thread has no
  Tokio context.
- `gadget_message`: same `spawn_blocking` rationale for
  `handle-message` RPC.

## Host dispatch: `GadgetHost::search`

`src-tauri/src/gadget_host.rs`

Two branches: prefix-exclusive and the normal catalog + fan-out path.
Empty queries short-circuit by sending only `Done`.

### Prefix-exclusive routing (ADR 0012)

`find_prefix_match` walks every active gadget slot, examining each
prefix declared by `Gadget::search_prefixes()`
(`gadgets/mod.rs`). The longest prefix that is a literal `starts_with`
match wins; ties are not possible because length is the sole key and
disabled slots are skipped.

When a prefix matches:

1. The matched prefix is stripped from the query.
2. The gadget's `search(stripped, Some(prefix))` runs on a single
   `tokio::task::spawn_blocking`.
3. The returned `GadgetResponse` is fed to `process_gadget_response`
   with `allow_custom_ui = true`.
4. Frecency bonuses are applied to the entries, then a stable
   score-DESC sort is used (not the full `cmp_sort_key`) so the
   gadget's intended ordering for equal-score items is preserved.
5. One `SearchResults` message is emitted with `matched_prefix:
   Some(prefix)`, followed by `Done`.

No catalog search runs. No other gadget runs. This is the "exclusive"
in ADR 0012's exclusive routing.

### Normal path: catalog + concurrent query gadgets (ADR 0023)

When no prefix matches, every active gadget participates regardless of
whether it also declares prefixes (ADR 0023).

#### Phase 1: catalog (synchronous, single batch)

`search_catalogs_static` in `gadget_host.rs`.

The host clones an `Arc<dyn Gadget>` for every active slot and offloads
to `spawn_blocking`. Inside the blocking task:

- A nucleo `Matcher` with `Config::DEFAULT { prefer_prefix: true }` and
  a smart-case, smart-normalization `Pattern` are built once.
- For each gadget, `entries()` is called and every `CatalogEntry` is
  scored title-first via `Pattern::indices`. On a title miss, the
  gadget's `keywords` are appended to the title and re-scored with
  `Pattern::score` (no positions kept).
- Successful matches produce a `SourcedEntry` with `title_positions`
  derived from the grapheme indices, and an empty
  `subtitle_positions`.
- `FrecencyStore::apply_scores` adds per-source bonuses to the slice
  this gadget contributed.
- The full vector is sorted by `SourcedEntry::cmp_sort_key`: score
  DESC, source ASC, id ASC.

The host always emits exactly one `SearchResults { source: Catalog,
... }` message even when the vector is empty. The frontend uses the
catalog batch as the canonical "new generation, recompute" signal.

#### Phase 2: query-gadget fan-out

Every active slot is dispatched in parallel via a `tokio::task::JoinSet`
of `spawn_blocking` tasks. Each task calls `gadget.search(query, None)`
(no `matched_prefix` because prefix routing already exclusive-shorted)
and yields `(source, response)`.

The host drains the `JoinSet` with `join_next().await`. Order of
arrival is non-deterministic. Per response:

- `None` → skip (catalog-only or query-but-not-relevant gadgets).
- `Some(response)` → `process_gadget_response(..., allow_custom_ui:
  false)`, frecency bonus, sort by `cmp_sort_key`, emit
  `SearchResults`.

An empty entry list is still emitted so the per-source key on the
frontend uses the message to evict that source's prior entries.

After the `JoinSet` drains, the host emits `Done`.

### Concurrency model

- Catalog phase: one `spawn_blocking` task. Blocks Phase 2 from
  starting because the await is sequential.
- Query phase: N concurrent `spawn_blocking` tasks (one per active
  gadget) on Tokio's blocking pool. The host streams results back as
  each task completes, with no all-or-nothing barrier.
- The blocking pool is mandatory for the same Tokio-runtime-context
  reason as `search_execute`: WASM gadgets reach host imports like
  `http::fetch` and `command::run` from inside `search()`.

### Stale-call elision (WASM only)

`src-tauri/src/wasm/runtime/instance.rs`.

WASM components are single-threaded by spec, so every guest export
serializes on the per-instance store mutex. When a guest blocks
inside a host import (e.g. `website-metadata::lookup` in `Blocking`
mode), every later keystroke's `search()` call queues behind it.
Without elision, FIFO would drain the entire backlog after the slow
call finishes, wasting compute on results the frontend would
discard by generation anyway.

`WasmGadgetInstance::search` increments
`search_generation: AtomicU64` on entry, then re-checks the counter
*after* acquiring the store lock. If a newer call has registered
during the wait, the older call returns
`GadgetResponse::Results(vec![])` without invoking the guest.

Native gadgets do not have this problem and do not need this gate.

## `process_gadget_response`

`gadget_host.rs`.

Translates `GadgetResponse` (host-internal, in
`commands/types.rs`) into
`(Option<(ViewKind, GadgetViewRef)>, Vec<SourcedEntry>)`:

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

In the fan-out path only one gadget can claim the inline slot per
search. The host tracks `inline_claimed: bool` across the
`join_next` loop. The first arriving `InlineUI` wins; later inline
view refs are dropped with a warning. Because completion order is
non-deterministic, the winner is non-deterministic.

The prefix path is single-gadget so the contention question does not
arise there.

## Sort comparator

The key `(score DESC, source ASC, id ASC)` is implemented in:

- Rust catalog phase: `cmp_sort_key`.
- Rust fan-out phase: `cmp_sort_key`.
- Rust prefix phase uses score-DESC only, not the full key; see the
  comment in the prefix path for the reason.
- `SourcedEntry::cmp_sort_key`: `commands/types.rs`.
- TypeScript: `src/launcher/compareEntries.ts`, used in `useSearch.ts`
  and in the launcher's selection-stability binary search
  (`src/launcher/Launcher.tsx`).

The frontend relies on each backend batch arriving pre-sorted. The
contract is implicit (no type enforces it).

## EntryStore: search-to-execute handoff

`EntryStore` (`src-tauri/src/entry_store.rs`) bridges the gap between
the search phase and the execute phase. It is a
`RwLock<HashMap<(String, String), ScoredEntry>>` keyed by
`(source_gadget_id, entry_id)`.

- **During search**, `entry_store.clear()` runs unconditionally at the
  top of every `GadgetHost::search` invocation. As each batch of
  results (catalog or per-query-gadget) is produced, the entries are
  inserted into the store.
- **During execute**, `entry_store.get(source, entry_id)` retrieves
  the full `ScoredEntry` to pass to the gadget's `execute()`. This is
  how the opaque `data` field round-trips from `search()` back to
  `execute()` without the frontend ever seeing it (`data` is
  `#[serde(skip)]` on the host `ScoredEntry`). If the entry is absent
  (stale UI referencing a previous search generation), the host logs
  the mismatch and returns `PostAction::Nothing`.

See [02-data-types.md](02-data-types.md#execution) for the full
execute type contract.

## Frecency

Frecency is a host service. Each gadget slot has a scoped
`GadgetFrecency` handle. After a gadget's results are scored, the host
calls `FrecencyStore::apply_scores(source, &mut entries)` to add per-id
bonuses to that gadget's slice. Gadgets do nothing.

Recording happens on the *execute* side: `GadgetHost::execute` calls
`self.frecency.record(source, entry_id)` *before* dispatching to the
gadget, so user intent is captured even when the action errors.

The WIT `frecency` interface (`gadget-sdk/wit/torchsnap-gadget.wit`)
is purely read-side: it lets a gadget query its own top items to
drive UI like the emoji picker's empty-query browse mode. It is not
involved in the search-result-bonus path.

## Gadget contract

Native gadgets implement `crate::gadgets::Gadget`
(`gadgets/mod.rs`). The search-relevant entry points:

- `id() -> &str`: the `source` field on every emitted entry.
- `search_prefixes() -> &[String]`: empty by default.
- `entries() -> Vec<CatalogEntry>`: empty by default (query-only
  gadgets).
- `search(query, matched_prefix) -> Option<GadgetResponse>`: `None`
  by default (catalog-only gadgets). `matched_prefix.is_some()` iff
  this is the prefix-exclusive path.
- `execute(entry: &ScoredEntry, action_id: &ActionId) ->
  anyhow::Result<PostAction>`.

WASM gadgets implement the WIT `search` interface
(`gadget-sdk/wit/torchsnap-gadget.wit`) and reach the host through
`WasmGadgetBridge` (`wasm/bridge.rs`), which adapts the WIT types to
the native `Gadget` trait. The bridge's `search_prefixes()` reads
`manifest.gadget.prefixes` from `manifest.toml` rather than a guest
call, so prefix matching does not touch the WASM store mutex.

WIT differences worth noting:

- WIT `search-response` has a `nothing` variant that the bridge maps
  to `Gadget::search` returning `None`. An empty
  `Results(vec![])` is distinct: it means "I am participating but
  have no entries", and is forwarded so per-source eviction works.
- WIT `scored-entry` carries `title-highlight-positions` and
  `subtitle-highlight-positions` as UTF-16 offsets, which match the
  host's `Utf16Positions`.
- WIT `scored-entry` includes `data: option<string>` for opaque
  payload round-tripping through `execute`.

## Sample gadget shapes

- `gadgets/bangs/`: query gadget. Demonstrates prefix-free
  search with `!`-prefixed bang tokens detected inside the query text.
- `gadgets/calculator/`: query gadget with a prefix (`=`).
  Demonstrates prefix-exclusive routing with custom UI (history view)
  and inline UI (heuristic result above the list).
- `gadgets/emoji-picker/`: query gadget with a prefix (`:`).
  Uses custom UI (`EmojiGrid` view) and `frecency::top-items` for
  an empty-query browse mode. Returns empty `entries()`.
- `gadgets/hello-world/`: hybrid gadget with one catalog entry (`"greet"`)
  plus `search()` for prefix mode (`!` → custom echo view) and
  fuzzy petname matching in the non-prefix path.

## Custom vs inline UI vs structured results (ADR 0021)

Three response shapes flow through the same `SearchResults` message:

| Variant | Where allowed | Frontend effect |
|---|---|---|
| `Results` | always | merged into the result list |
| `InlineUI` | always (one-per-search winner) | mounts an inline component above the list; entries also merged |
| `CustomUI` | prefix mode only | replaces the list wholesale with the gadget's view |

`GadgetViewRef` (`commands/types.rs`) carries `gadgetId`,
`view`, and optional JSON `data`. The frontend uses `gadgetId` to
look up the React component in its gadget registry; `view` selects
between `views[view]` (custom) or `inlineViews[view]` (inline).
See [04-frontend-reception.md](04-frontend-reception.md) for how
these messages are received, accumulated, and rendered.

## Cancellation semantics

The Tauri command future is awaited on the host side; if the
frontend tears down (e.g. the launcher closes), the channel sender
errors are silently dropped (`let _ = on_results.send(...)`). Tasks
already running on the blocking pool are not cancelled. They run to
completion and their messages are then discarded by the frontend's
generation guard. The WASM stale-elision path is the only place that
actively avoids redundant work; everywhere else, "cancellation" means
"the result is discarded after the fact".
