# Gadget Data Types

The canonical names live in
`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`. Where the Rust host
keeps a parallel definition (richer or differently shaped), it lives
in `src-tauri/src/commands/types.rs`. The frontend has equivalent
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
| Action | `types.action` | `Action<C>` |
| Actions of an entry | `types.entry-actions` | `EntryActions<C>` |
| Action slot | (the fields of `entry-actions`) | `Slot` |
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
  asset-icon(string),  // path relative to gadget archive root (gadget-produced),
                       // or absolute path (host-produced, e.g. website-metadata favicons)
  emoji(string),       // single Unicode emoji rendered as text
  app-icon(string),    // platform application identifier, e.g. a macOS bundle id
}
```

The host mirrors this 1:1 in `EntryIcon`. `asset-icon` is what the
website-metadata service returns for favicons (as absolute paths) and
what gadgets use for bundled icon assets (as relative paths within
their archive). `app-icon` requires the `icon-cache` permission in the
manifest; the WASM response pass resolves it to an `asset-icon`
pointing at the cached image, or drops the icon if the application
cannot be found or the permission is missing (`resolve_entry_icon` in
`src-tauri/src/wasm/bindings.rs`). `app-icon` never reaches the
frontend as its own variant.

### `action`

```wit
record action {
  label: option<string>,
  command: string,
}
```

`command` is a gadget-defined value that says what running the
action does. The host stores it with the entry, never inspects it,
and hands it back unchanged to `execute()` when the user runs the
action. Its encoding is up to the gadget. The Rust SDK encodes the
gadget's `Command` type as JSON.

`label` is what the launcher shows. The `primary` and `secondary`
slots require one, and the launcher falls back to the entry's title
when it is missing there. For the other slots, `none` shows the
slot's default label.

### `entry-actions`

```wit
record entry-actions {
  primary: option<action>,        // Enter, or a click on the entry
  secondary: option<action>,      // Cmd+Enter
  copy: option<action>,           // Cmd+C
  reveal: option<action>,         // Cmd+Shift+R
  delete: option<action>,         // Cmd+Backspace
  open-settings: option<action>,  // Cmd+,
}
```

The slot alone decides how the user runs an action. The order of
the fields has no meaning, and an empty slot has no key and no
footer hint. The same command may fill several slots. Outside macOS,
Ctrl takes the place of Cmd. The default labels for `none` are
"Copy", "Reveal in Finder", "Delete" and "Open settings", the same
English text on every platform. The frontend owns the table from
slot to key and default label (`src/launcher/actionSlots.ts`).

The host mirrors both records as `Action<C>` and `EntryActions<C>`
(`src-tauri/src/commands/types.rs`), generic over the command type.
The host holds commands as `String`. Native gadgets use their own
type through the typed `Search` trait, which encodes it to a
`String`. `EntryActions` serializes to the frontend as a list of
`{ slot, label }` in slot order, without the commands, and `Slot`
serializes in camelCase (`"openSettings"`).

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
  actions: entry-actions,
}
```

`keywords` is the catalog-only fallback match target: scored against
`title + keywords`, but match positions on keyword hits are not
highlighted. The host `CatalogEntry<C>`
(`src-tauri/src/commands/types.rs`) has the same fields, with
`actions: EntryActions<C>`.

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
  actions: entry-actions,
}
```

Anything `execute()` needs travels in the commands of the entry's
actions. The host `ScoredEntry<C>` carries them in
`actions: EntryActions<C>`, which never serializes a command to the
frontend.

The host `ScoredEntry` uses a `Utf16Positions` newtype around the
highlight offsets and implements `FrecencyTarget` so the host can
apply frecency bonuses after the gadget returns.

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
`nothing`. Returning `results([])` is semantically different: it
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

`search::execute(command: string) -> result<post-action, string>`.

`command` is the `command` of the action the user triggered, exactly
as the gadget returned it. The gadget receives neither the entry nor
the slot. The frontend's `search_execute(source, entryId, slot)`
names the entry and the slot. The host looks up the entry of the
current search by `(source, entryId)` and passes the command in that
slot. If the entry is gone or the slot is empty, the host logs it
and returns `Nothing` without calling the gadget.

The host's `ErasedSearch` trait (a supertrait of `Gadget`) mirrors
this with `execute(&self, command: &str) ->
anyhow::Result<PostAction>`. Its typed counterpart `Search` has
`execute(&self, command: Self::Command)`, and a blanket impl decodes
the JSON string into `Self::Command` before calling it.

```wit
enum post-action {
  nothing,
  dismiss,
  keep-open,
  open-settings,
}
```

`nothing` leaves the launcher state unchanged, `dismiss` hides it,
`keep-open` is the multi-select escape hatch. `open-settings` hides the
launcher and opens the Settings window on the executing gadget's
section (ADR 0054). WASM gadgets cannot
trigger custom-UI takeover from `execute`. That path exists only on
the host `PostAction` enum (`PostAction::ShowCustomUI { view, data
}`) and is reachable only from native gadgets. WASM gadgets request
custom UI through `search-response::custom-ui` instead.

The `Err(string)` arm surfaces a guest-reported failure to the host
log; bridge-level failures (linker, store lock, serialization) surface
separately so the two are distinguishable in logs.

## Frontend wire types

The host augments scored entries with their source gadget and
streams them to the frontend over a Tauri channel. These types are
serialize-only on the Rust side (defined in
`src-tauri/src/commands/types.rs`); the launcher TypeScript has hand-
maintained mirrors.

### `SourcedEntry`

```rust
pub struct SourcedEntry {
    pub source: String,        // gadget id
    #[serde(flatten)]
    pub inner: ScoredEntry,
}
```

Gadgets never set `source`. The host attaches it via
`SourcedEntry::new(source, scored)`, preventing one gadget from
spoofing another gadget's id. Sort order is `score DESC, source ASC, id ASC` via
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
providing gadget, not per-gadget: it replaces the catalog layer
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

Both `payload` and the `Ok` arm are JSON-encoded strings. The
gadget parses with `serde_json::from_str` on the way in and
serializes the response on the way out. The Rust SDK's `Messaging`
trait does both: it decodes each call into the gadget's `Request`
enum and serializes the `serde_json::Value` that `handle()` returns
(see [05-gadget-messaging.md](05-gadget-messaging.md#rust-sdk)).
Strict request/response;
WASM gadgets cannot stream. Native gadgets still receive a streaming
`Channel<Value>` via `Gadget::handle_message`; only the WIT bridge
is one-shot.

Returning `Err(string)` propagates as a Tauri command error visible
to the frontend's `sendMessage` promise.

## Scheduled tasks

### `tasks::run-task(task-id) -> result<_, string>`

Invoked by the host's per-gadget scheduler at the cron times
declared in `manifest.toml` `[[tasks]]`. `task-id` matches the
manifest entry's `id`. `Err(string)` is logged but does not disable
the gadget. Scheduled tasks are best-effort background work, not
lifecycle-critical. Gadgets without `[[tasks]]` provide a no-op
implementation via `impl_noop_tasks!`; the host never spawns a
scheduler loop in that case.

## Known gaps

- Highlight positions: the WIT contract says UTF-16 code unit
  offsets (`title-highlight-positions`,
  `subtitle-highlight-positions`). The Rust host wraps them in a
  `Utf16Positions` newtype. The TypeScript frontend treats them as
  JS string indices, which is correct for UTF-16, but historical
  TODOs in the codebase noted nucleo grapheme-index issues; verify
  with the current scoring pipeline if you touch highlighting.
- Type sync: WIT records, host Rust structs, and frontend TS types
  are kept in sync by hand. There is no code generation across the
  Rust / TS boundary.
