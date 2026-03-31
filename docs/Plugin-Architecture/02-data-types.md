# Plugin Data Types and Their Flow

## The Type Chain

Five distinct "result" types exist in the pipeline, forming a chain from
plugin output to frontend consumption:

```
CatalogEntry ──┐
               ├─→ PluginResponse ──→ ScoredEntry ──→ SearchMessage
QueryResult ───┘
```

### CatalogEntry (`search/types.rs`)

Internal only, never serialized. Produced by `CatalogPlugin::entries()`.

```rust
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub keywords: Vec<String>,   // match targets not shown in UI
    pub actions: Vec<Action>,
}
```

The `keywords` field is unique to this type — it enables fallback matching
where the host scores against `title + keywords` if the title alone doesn't
match. This feature has no counterpart for query plugins.

### QueryResult (`search/types.rs`)

What `QueryPlugin` implementations produce. Structurally identical to
`CatalogEntry` minus `keywords`, plus pre-computed scoring fields:

```rust
pub struct QueryResult {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    pub title_positions: Vec<u32>,
    pub subtitle_positions: Vec<u32>,
    pub actions: Vec<Action>,
}
```

### PluginResponse (`search/types.rs`)

Internal channel message between a plugin's blocking thread and the host's
async task. Three variants:

```rust
pub enum PluginResponse {
    Results(Vec<QueryResult>),
    CustomUI { view: String, data: Option<Value>, results: Vec<QueryResult> },
    InlineUI { view: String, data: Option<Value>, results: Vec<QueryResult> },
}
```

Only `QueryPlugin` can produce `CustomUI` or `InlineUI` responses.
`CatalogPlugin` can only trigger custom UI via `PostAction::ShowCustomUI` from
`execute()`, which follows a completely different path.

### ScoredEntry (`search/types.rs`)

The bridge type — serialized to the frontend. Identical to `QueryResult` plus
`source: String` (the plugin ID). The conversion via
`QueryResult::into_scored_entry()` copies all fields verbatim and adds `source`.

```rust
pub struct ScoredEntry {
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<EntryIcon>,
    pub score: u32,
    pub title_positions: Vec<u32>,
    pub subtitle_positions: Vec<u32>,
    pub source: String,
    pub actions: Vec<Action>,
}
```

### SearchMessage (`search/types.rs`)

The wire format sent over the Tauri IPC channel:

```rust
pub enum SearchMessage {
    SearchResults {
        entries: Vec<ScoredEntry>,
        custom_plugin_view: Option<PluginViewRef>,
        inline_plugin_view: Option<PluginViewRef>,
        matched_prefix: Option<String>,
    },
    Done,
}
```

### PluginViewRef (`search/types.rs`)

Carried inside `SearchMessage` when a plugin requests UI takeover:

```rust
pub struct PluginViewRef {
    pub plugin_id: String,
    pub view: String,        // key into frontend registry
    pub data: Option<Value>, // opaque payload
}
```

## Supporting Types

### EntryIcon

Tagged union for icon sources:
- `HeroIcon(String)` — Heroicons name
- `DataUrl(String)` — base64 data URL (currently `dead_code`)
- `AssetIcon(String)` — filesystem path via Tauri asset protocol
- `Emoji(String)` — Unicode emoji character

### PostAction

Return value of `execute()`, controls post-action behavior:
- `Nothing` — do nothing
- `Dismiss` — hide the launcher
- `KeepOpen` — currently `dead_code`
- `ShowCustomUI` — enter custom plugin view

### ActionId

Tagged enum for action identification, bidirectional across the bridge:
- Standard: `Open`, `Copy`, `Reveal`, `OpenWith`, `Delete`
- `Custom(String)` — plugin-defined actions

## Known Issues

- `title_positions` are nucleo grapheme indices, not JS string indices. Emoji
  and CJK character highlighting is broken (documented TODO in `types.rs`).
- `QueryResult` and `ScoredEntry` are structurally identical minus `source`,
  creating mechanical field-copying boilerplate.
- Types are manually mirrored between Rust (`search/types.rs`) and TypeScript
  (`launcher/types.ts`) with no code generation ensuring sync.
