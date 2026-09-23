# Type-safe execute data via SDK associated type

**Status: needs discussion** — depends on the opaque `data`
field, already landed as `ScoredEntry::data`.

## Problem

Once `scored-entry` carries an opaque `data: option<string>`
and `execute()` receives it back, the connection between the
type serialized in `search()` and the type deserialized in
`execute()` is implicit. Nothing prevents a gadget from
encoding `FooData` in search and attempting to decode `BarData`
in execute — the mismatch only surfaces at runtime as a
deserialization error.

## Proposed solution

Introduce an associated type on a wrapper trait in the gadget
SDK that enforces the search↔execute data contract at compile
time. The WIT layer stays opaque (`option<string>`); the SDK
handles serialization transparently.

### SDK trait

```rust
/// Replaces the raw WIT-generated `SearchGuest` for gadget
/// authors. `define_gadget!` generates the WIT shim that
/// bridges between this trait and the generated one.
trait GadgetSearch {
    /// The per-entry data type round-tripped through the host.
    /// Use `()` for gadgets that don't need execute-time data.
    type Data: Serialize + DeserializeOwned;

    fn entries() -> Vec<CatalogEntry>;

    fn search(
        query: String,
        matched_prefix: Option<String>,
    ) -> SearchResponse<Self::Data>;

    fn execute(
        entry_id: String,
        action_id: ActionId,
        data: Option<Self::Data>,
    ) -> Result<PostAction, String>;
}
```

### `ScoredEntry<D>` becomes generic

```rust
struct ScoredEntry<D = ()> {
    id: String,
    title: String,
    subtitle: Option<String>,
    icon: Option<EntryIcon>,
    score: u32,
    title_highlight_positions: Vec<u32>,
    subtitle_highlight_positions: Vec<u32>,
    actions: Vec<Action>,
    data: Option<D>,
}
```

`SearchResponse<D>` wraps `Vec<ScoredEntry<D>>` in the
`Results` variant. The `CustomUi`/`InlineUi` variants carry
the same generic.

### Macro-generated shim

`define_gadget!` expands to an impl of the WIT-generated
`SearchGuest` that:

1. Calls `<MyPlugin as GadgetSearch>::search(...)`.
2. Maps each `ScoredEntry<D>` → WIT `ScoredEntry` by calling
   `serde_json::to_string(&entry.data)` for the `data` field.
3. In `execute()`, deserializes the incoming
   `Option<String>` → `Option<D>` via
   `serde_json::from_str`, then delegates to
   `<MyPlugin as GadgetSearch>::execute(entry_id, action_id, data)`.

### `type Data = ()` for gadgets without data

Gadgets that don't need execute-time data (calculator,
emoji-picker, hello-world) set `type Data = ()`. The shim
skips serialization when `D = ()` — the WIT `data` field
stays `none`, and `execute()` receives `None`. No runtime
cost, no boilerplate.

### Gadget author experience

```rust
#[derive(Serialize, Deserialize)]
struct BangData { url: String }

impl GadgetSearch for BangsPlugin {
    type Data = BangData;

    fn search(query: String, _prefix: Option<String>) -> SearchResponse<BangData> {
        // ...
        SearchResponse::Results(vec![ScoredEntry {
            data: Some(BangData { url: resolved_url }),
            ..Default::default()
        }])
    }

    fn execute(
        _entry_id: String,
        action_id: ActionId,
        data: Option<BangData>,
    ) -> Result<PostAction, String> {
        let bang = data.ok_or("no data attached to entry")?;
        opener::open_url(&bang.url)?;
        Ok(PostAction::Dismiss)
    }
}
```

Changing `BangData`'s fields forces both `search()` and
`execute()` to update — the compiler catches mismatches.

## Open questions

- Should `GadgetSearch` replace `SearchGuest` entirely in
  the public SDK surface, or coexist as an opt-in layer? If
  a gadget wants raw access to the WIT `data` string (e.g.
  for a non-JSON encoding), the wrapper trait gets in the
  way.

- The generic `ScoredEntry<D>` may complicate type inference
  in gadgets that build entries across multiple helper
  functions. An alternative is to keep `ScoredEntry`
  non-generic and use a builder method
  (`entry.with_data(&bang_data)`) that returns an
  `EntryWithData<D>` wrapper — uglier but avoids infecting
  every helper with the generic.

- `SearchResponse<D>` has variants (`CustomUi`, `InlineUi`)
  whose `results` lists would also carry `D`. Verify that
  the generic propagates cleanly through all variants and
  the `ViewResponse` record.

## Related

- `ScoredEntry::data` in `src-tauri/src/commands/types.rs`: the
  opaque `data` field this builds on.
- `gadgets/gadget-sdk/src/lib.rs` — `define_gadget!` macro
  and SDK trait re-exports.
- `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` — WIT types.
