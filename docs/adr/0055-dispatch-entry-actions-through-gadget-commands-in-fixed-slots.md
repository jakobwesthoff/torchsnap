# 55. Dispatch entry actions through gadget commands in fixed slots

Date: 2026-09-25

## Status

Accepted

Amends [9. Use multi-action model with action palette for result entries](0009-use-multi-action-model-with-action-palette-for-result-entries.md)

Amends [13. Allow plugins to provide custom UI components for the result area](0013-allow-plugins-to-provide-custom-ui-components-for-the-result-area.md)

Amends [24. Unify CatalogPlugin and QueryPlugin into a single Plugin trait](0024-unify-catalogplugin-and-queryplugin-into-single-plugin-trait.md)

Amends [41. ZeroTier plugin architecture](0041-zerotier-plugin.md) (replaces the failure entry's `ActionId::OpenSettings` with a `primary` action)

Amends [54. Open gadget settings through a post-action](0054-open-gadget-settings-through-a-post-action.md)

Amended by [56. Version the WIT contract and both gadget SDKs in lockstep](0056-version-the-wit-contract-and-both-gadget-sdks-in-lockstep.md) (the SDKs get the WIT's new version too)

## Context

Each action on a result entry has an `ActionId` and a label. The
`ActionId` variants are `open`, `copy`, `reveal`, `open-with`, `delete`,
`open-settings` and `custom(string)`. The id does two jobs. The
frontend sends it back in `search_execute`, and `execute()` matches on
it to learn which action the user chose. The host also derives a
default keybinding from it for WASM gadgets: Cmd+C for `copy`,
Cmd+Shift+R for `reveal`, Cmd+Shift+O for `open-with`, Cmd+Backspace
for `delete`, Cmd+, for `open-settings`, none for `open` and `custom`.

Enter and a click on the row run the first action in the list,
whatever its id. A secondary action is reachable only if it has a
keybinding.

`execute()` receives the stored entry and the action id. A scored
entry can carry an opaque `data: option<string>`, which the Rust SDK's
`data::encode` and `data::decode` fill with JSON. Nothing ties the type
encoded in `search()` to the type decoded in `execute()`. Catalog
entries have no `data` field. Gadgets without `data` derive their
payload from the entry id: zerotier parses `network:<id>`, app launcher
and system preferences treat the id as a path or pane id, and
calculator copies the id to the clipboard.

In the gadgets of this repository:

- No action uses `custom`. Every primary action uses `open`, with labels
  such as "Quit", "Lock", "Join" and "Eject".
- No action uses `open-with`. The `opener` capability opens a path only
  with the application the OS has registered for it.
- App launcher's "Reveal in Finder" sets Cmd+Enter explicitly, while
  WASM gadgets get Cmd+Shift+R for `reveal`.
- The calculator views call `onExecute` with the computed result as the
  entry id. The host finds no stored entry with that id, so nothing is
  copied.

The gadget API may change incompatibly before the 1.0.0 release, and no
external gadgets exist yet.

## Decision

### Commands

Each action carries a command. A command is a gadget-defined value that
the WIT carries as an opaque `string`. The host stores it with the
entry and hands it back unchanged, and never inspects it. The Rust SDK
encodes commands as JSON. Gadgets in other languages choose their own
encoding.

`execute()` receives only the command of the action the user
triggered, not the entry and not an action id.

`scored-entry.data` is removed. The SDK's `data` module leaves the
prelude and stays internal to the SDK. `view-response.data`, the JSON
handed to a gadget's view component, is unchanged.

The entry id stays. The host uses it for frecency and for the
entry-store lookup, and the frontend uses it as list key and sort
tiebreaker. Gadgets no longer read it in `execute()`.

### Slots

An entry's actions are a record with one optional field per slot:

```wit
record action {
  /// Required for `primary` and `secondary`. For the other slots the
  /// launcher shows a default label when this is `none`.
  label: option<string>,
  /// Opaque to the host; returned unchanged to `execute()`.
  command: string,
}

record entry-actions {
  primary: option<action>,
  secondary: option<action>,
  copy: option<action>,
  reveal: option<action>,
  delete: option<action>,
  open-settings: option<action>,
}
```

| Slot | Key | Label |
|---|---|---|
| `primary` | Enter, click on the row | required |
| `secondary` | Cmd+Enter | required |
| `copy` | Cmd+C | optional, default "Copy" |
| `reveal` | Cmd+Shift+R | optional, default "Reveal in Finder" |
| `delete` | Cmd+Backspace | optional, default "Delete" |
| `open-settings` | Cmd+, | optional, default "Open settings" |

The default labels are the same English text on every platform.

The slot alone decides the key. The order of fields has no meaning, and
an empty slot has no key and no footer hint. The same command may fill
several slots, for example `primary` and `copy` on an entry whose main
action is copying.

A `primary` or `secondary` action without a label is shown with the
entry's title.

`catalog-entry` and `scored-entry` both carry `entry-actions`.

`ActionId` is removed with all its variants, including `open`,
`custom` and `open-with`. This slot set is complete until gadget-defined
bindings are designed (see "Deferred").

### Frontend

The frontend owns the table from slot to key and from slot to default
label. `Action` loses its `keybinding` field. For each entry, the host
sends the filled slots as a list of `{ slot, label }` in the order of
the table above. Commands never reach the frontend.

`search_execute` sends `(source, entryId, slot)`. The host looks up the
stored entry by `(source, entryId)` and passes the command in that slot
to the gadget. If the slot is empty, the host logs it and returns
`Nothing`.

A custom or inline view runs an action of an entry it received in
`results` with `onExecute(entryId, slot)`. A view that acts on its own
state calls its backend with `sendMessage` and then a launcher action
such as `dismiss()`.

`LauncherActions` gains `openSettings()`. Like the `open-settings`
post-action, it hides the launcher and opens the Settings window on the
gadget's section. The launcher supplies the view's gadget id, so a view
opens only its own gadget's settings section. Every launcher effect that a WIT `post-action` variant
triggers is also available as a `LauncherActions` function.

### Rust gadget SDK

The SDK gets a typed trait with a blanket impl of the generated
`SearchGuest`:

```rust
pub trait Search {
    type Command: Serialize + DeserializeOwned;

    fn entries() -> Vec<CatalogEntry<Self::Command>>;
    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse<Self::Command>;
    fn execute(command: Self::Command) -> Result<PostAction, String>;
}

impl<T: Search> SearchGuest for T { /* encodes and decodes commands */ }
```

- The SDK owns `CatalogEntry<C>`, `ScoredEntry<C>`, `SearchResponse<C>`,
  `ViewResponse<C>` and `Actions<C>`, and the prelude exports these
  instead of the generated types.
- `Actions<C>` is a struct with public `Option` fields and `Default`,
  plus builder methods and an `iter()` over the filled slots. The
  builder takes a label for `primary` and `secondary`.
- `SearchGuest` stays exported from the crate root but not from the
  prelude. A gadget may still implement it by hand. A type that
  implements both `Search` and `SearchGuest` does not compile (E0119).
- A command that fails to decode in `execute()` becomes an `Err` before
  gadget code runs. A command that fails to encode in `search()` or
  `entries()` is logged as a warning, and its entry is dropped.
- `define_gadget!` keeps expanding to `export!` only.

Messaging gets a typed layer in the same refactor, with the same
pattern: a `Messaging` trait with a `Request` enum and a blanket impl of
`MessagingGuest`. The WIT `handle-message` and the frontend's
`sendMessage` stay as they are, and launcher views and settings panels
keep sharing one message channel. For methods without arguments, the
frontend's `{}` payload is accepted by a unit variant.

### Host

Search moves out of `Gadget` into two traits:

- `ErasedSearch`: `entries()`, `search()` and
  `execute(&self, command: &str)`. `Gadget` has `ErasedSearch` as a
  supertrait, so the host keeps holding `Arc<dyn Gadget>`.
- `Search`: the typed counterpart with `type Command`, and a blanket
  impl of `ErasedSearch` that encodes and decodes the commands as JSON.

Native gadgets implement `Gadget` and `Search`. The WASM bridge
implements `ErasedSearch` directly and passes command strings through.

### Deferred

Gadget-defined actions with their own bindings are not part of this
decision and will be designed separately
(`todos/product/features/01kmh12n6r0mq94rwav32eczdn-keybind-system.md`).

## Consequences

The WIT changes incompatibly, and the package version goes from
`torchsnap:gadget@0.1.0` to `torchsnap:gadget@0.2.0`. Every gadget in
this repository, WASM and native, needs code changes, and WASM gadgets
need a rebuild.

From ADR 9, the rule that the first action is the primary one, the
positional modifier tiers (Cmd+Enter, Alt+Enter, Shift+Enter for the
second to fourth action) and the `custom` and `open-with` action ids no
longer apply. Cmd+Enter belongs to the `secondary` slot. App launcher's
"Reveal in Finder" moves into `secondary` and keeps Cmd+Enter.

From ADR 13, the view callback `onExecute(entryId, actionId)` becomes
`onExecute(entryId, slot)`.

From ADR 24, `Plugin::execute(entry_id, action_id, app)` is replaced by
`ErasedSearch::execute(command)`, and `entries()` and `search()` move
from `Gadget` to `ErasedSearch`.

From ADR 41, zerotier's "token not configured" and "authentication
failed" entries carry a `primary` action labeled "Open settings",
whose command returns the `open-settings` post-action. They do not fill
the `open-settings` slot.

From ADR 54, the `OpenSettings` action becomes the `open-settings`
slot, whose command reaches `execute()` like any other. The
`open-settings` post-action is unchanged.

The developer documentation in torchsnap-docs describes `ActionId`,
`custom(string)` and the `data` field, and needs updating along with
the code.
