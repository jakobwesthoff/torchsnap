# Gadget Data Types

The canonical names live in
`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`. Where the Rust host
keeps a parallel definition (richer or differently shaped), it lives
in `src-tauri/src/search/types.rs`. The frontend has equivalent
TypeScript copies that are kept in sync by hand.

This document covers the types gadgets exchange with the host:
search results and their wrappers, action descriptors, icons,
post-action behavior, and ancillary settings shapes.

## Where the types live

| Concept | WIT (guest-visible) | Host Rust |
|---|---|---|
| Catalog entry | `search.catalog-entry` | `CatalogEntry` |
| Pre-scored entry | `search.scored-entry` | `ScoredEntry` |
| Inline / custom UI payload | `search.view-response` | inside `GadgetResponse::CustomUI` / `InlineUI` |
| Search return value | `search.search-response` | `Option<GadgetResponse>` |
| Action | `types.action` | `Action` (adds `keybinding`) |
| Action ID | `types.action-id` | `ActionId` |
| Icon | `types.entry-icon` | `EntryIcon` |
| Post-action | `search.post-action` | `PostAction` (adds `ShowCustomUI`) |

## Shared types (`types` interface)

Defined in their own interface so host imports and guest exports can
both `use` them and resolve to the same generated Rust type.

### `entry-icon`

```wit
variant entry-icon {
  hero-icon(string),   // Heroicons name, e.g. "x-circle"
  data-url(string),    // base64-encoded inline image
  asset-icon(string),  // absolute filesystem path served via Tauri's asset protocol
  emoji(string),       // single Unicode emoji rendered as text
}
```

The host mirrors this 1:1 in `EntryIcon`. `data-url` is currently
not constructed by any shipping gadget (`#[allow(dead_code)]`).
`asset-icon` is what the website-metadata service returns for
favicons.

### `action-id`

```wit
variant action-id {
  open,
  copy,
  reveal,
  open-with,
  delete,
  open-settings,
  custom(string),
}
```

The well-known variants exist so the host can attach default
keybindings, icons, and labels. `open-settings` jumps to the
originating gadget's own settings panel — useful as the primary
action on synthetic "configuration required" entries. `custom` is
the escape hatch for gadget-specific actions.

### `action`

```wit
record action {
  id: action-id,
  label: string,
}
```

The host `Action` adds an optional `keybinding: ActionKeybinding`
({ `modifiers: Vec<String>`, `key: String` }) which the host
attaches based on the well-known `action-id`. Gadgets never set it
themselves; the WIT record has no field for it.

## Catalog mode

`search::entries() -> list<catalog-entry>`. Returned by gadgets that
hand the host a finite, pre-known list. The host runs nucleo fuzzy
matching and produces `scored-entry`-shaped results internally.

```wit
record catalog-entry {
  id: string,
  title: string,
  subtitle: option<string>,
  icon: option<entry-icon>,
  keywords: list<string>,
  actions: list<action>,
}
```

`keywords` is the catalog-only fallback match target: scored against
`title + keywords`, but match positions on keyword hits are not
highlighted. The host `CatalogEntry`
(`src-tauri/src/search/types.rs:150`) is identical apart from the
`keywords` field carrying a comment that the first action is the
primary one (Enter activation).

## Query mode

`search::search(query, matched-prefix) -> search-response`. Called on
every keystroke. The gadget scores its own results.

```wit
record scored-entry {
  id: string,
  title: string,
  subtitle: option<string>,
  icon: option<entry-icon>,
  score: u32,
  title-highlight-positions: list<u32>,    // UTF-16 code unit offsets
  subtitle-highlight-positions: list<u32>, // UTF-16 code unit offsets
  actions: list<action>,
}
```

The host `ScoredEntry` uses a `Utf16Positions` newtype around the
same offsets and implements `FrecencyTarget` so the host can apply
frecency bonuses after the gadget returns.

### `search-response`

```wit
variant search-response {
  nothing,                  // gadget has nothing for this query
  results(list<scored-entry>),
  custom-ui(view-response), // replaces the result list
  inline-ui(view-response), // rendered above the result list
}
```

Gadgets that don't participate in this keystroke should return
`nothing`. Returning `results([])` is semantically different — it
says the gadget participated and explicitly produced zero results,
which evicts any stale per-source entries on the frontend. The
bridge maps `search-response` to `Option<GadgetResponse>`:

| WIT | Host |
|---|---|
| `nothing` | `None` |
| `results(xs)` | `Some(GadgetResponse::Results(xs))` |
| `custom-ui(v)` | `Some(GadgetResponse::CustomUI { view, data, results })` |
| `inline-ui(v)` | `Some(GadgetResponse::InlineUI { view, data, results })` |

Only one gadget can claim the inline-ui slot per search. See
ADR 0021 for the structured-variants design and ADR 0008 for the
two-tier display rationale.

### `view-response`

```wit
record view-response {
  view: string,            // name registered in the gadget's frontend manifest
  data: option<string>,    // JSON-encoded opaque payload forwarded to the view
  results: list<scored-entry>,
}
```

`view` is resolved by name against the gadget's component registry
on the frontend (`registry[gadgetId].views[view]` for `custom-ui`,
`registry[gadgetId].inlineViews[view]` for `inline-ui`; see
ADR 0022). `data` is forwarded as-is to the React component; the
gadget's frontend parses the JSON itself. `results` carries the
scored entries the gadget still wants to display in the result list
(empty for full-takeover custom UIs).

WIT has no opaque JSON variant, so `data` is encoded as a string on
the wire. The host re-parses it into a `serde_json::Value` when
constructing `GadgetViewRef`.

## Execution

`search::execute(entry-id, action-id) -> result<post-action, string>`.

```wit
enum post-action {
  nothing,
  dismiss,
  keep-open,
}
```

`nothing` leaves the launcher state unchanged, `dismiss` hides it,
`keep-open` is the multi-select escape hatch. WASM gadgets cannot
trigger custom-UI takeover from `execute` — that path exists only on
the host `PostAction` enum (`PostAction::ShowCustomUI { view, data
}`) and is reachable only from native gadgets. WASM gadgets request
custom UI through `search-response::custom-ui` instead.

The `Err(string)` arm surfaces a guest-reported failure to the host
log with a `gadget error:` prefix; bridge-level failures (linker,
store lock, serialization) surface separately so the two are
distinguishable in logs.

## Frontend wire types

The host augments scored entries with their source gadget and
streams them to the frontend over a Tauri channel. These types are
serialize-only on the Rust side (defined in
`src-tauri/src/search/types.rs`); the launcher TypeScript has hand-
maintained mirrors.

### `SourcedEntry`

```rust
pub struct SourcedEntry {
    pub source: String,        // gadget id
    #[serde(flatten)]
    pub inner: ScoredEntry,
}
```

Gadgets never set `source` — `SourcedEntry::new(source, scored)`
attaches it host-side, preventing one gadget from spoofing another
gadget's id. Sort order is `score DESC, source ASC, id ASC` via
`cmp_sort_key`; the TS comparator in
`src/launcher/compareEntries.ts` mirrors this exactly.

### `ResultSource`

```rust
enum ResultSource {
  Gadget { id: String },
  Catalog,
}
```

`Catalog` is one aggregated batch mixing rows from every catalog-
providing gadget, not per-gadget — it replaces the catalog layer
wholesale on each query. `Gadget { id }` is one batch from one query
gadget.

### `GadgetViewRef`

```rust
pub struct GadgetViewRef {
    pub gadget_id: String,
    pub view: String,
    pub data: Option<serde_json::Value>,
}
```

Carried inside `SearchMessage` whenever a gadget returned
`custom-ui` or `inline-ui`.

### `SearchMessage`

```rust
enum SearchMessage {
  SearchResults {
    source: ResultSource,
    entries: Vec<SourcedEntry>,
    custom_gadget_view: Option<GadgetViewRef>,
    inline_gadget_view: Option<GadgetViewRef>,
    matched_prefix: Option<String>,
  },
  Done,
}
```

The frontend keys its accumulator by `source`, replaces that
source's batch on each new `SearchResults`, and stops when `Done`
arrives. `matched_prefix` is set when a prefix-routed query
exclusively reached this gadget (ADR 0012); the gadget's frontend
component needs it to know which trigger character was matched.

## Lifecycle and settings

### `lifecycle::on-setting-changed(key, value)`

Both arguments are strings: `key` is namespace-relative (the
`gadgets.<id>.` prefix is stripped before the call); `value` is a
JSON-encoded string matching the encoding `settings::get` uses on
the read side.

### `settings::get(key) -> option<string>`

Returns the JSON-encoded value or `none` if the key has never been
written (neither by a `[settings]` manifest default nor by a runtime
write). The gadget parses the JSON itself; the
`torchsnap-gadget-sdk` `settings::get<T>` /
`settings::get_or<T>` / `settings::get_or_else<T>` helpers
(`gadgets/gadget-sdk/src/settings.rs`) fold the parse step into the
call.

Settings defaults declared in `manifest.toml` `[settings]` are
seeded into the host store on first load (existing user values
never overwritten).

## Messaging (frontend RPC)

### `messaging::handle-message(method, payload) -> result<string, string>`

Both `payload` and the `Ok` arm are JSON-encoded strings — the
gadget parses with `serde_json::from_str` on the way in and
serializes the response on the way out. Strict request/response;
WASM gadgets cannot stream. Native gadgets still receive a streaming
`Channel<Value>` via `Plugin::handle_message`; only the WIT bridge
is one-shot.

Returning `Err(string)` propagates as a Tauri command error visible
to the frontend's `sendMessage` promise as `gadget error: <string>`.

## Scheduled tasks

### `tasks::run-task(task-id) -> result<_, string>`

Invoked by the host's per-gadget scheduler at the cron times
declared in `manifest.toml` `[[tasks]]`. `task-id` matches the
manifest entry's `id`. `Err(string)` is logged but does not disable
the gadget — scheduled tasks are best-effort background work, not
lifecycle-critical. Gadgets without `[[tasks]]` provide a no-op
implementation via `impl_noop_tasks!`; the host never spawns a
scheduler loop in that case.

## Known gaps

- Highlight positions: the WIT contract says UTF-16 code unit
  offsets (`title-highlight-positions`,
  `subtitle-highlight-positions`). The Rust host wraps them in a
  `Utf16Positions` newtype. The TypeScript frontend treats them as
  JS string indices, which is correct for UTF-16 — but historical
  TODOs in the codebase noted nucleo grapheme-index issues; verify
  with the current scoring pipeline if you touch highlighting.
- Type sync: WIT records, host Rust structs, and frontend TS types
  are kept in sync by hand. There is no code generation across the
  Rust / TS boundary.
