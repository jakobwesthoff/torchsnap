---
kind: refactor
status: open
---

# GadgetHost is a god object spanning five responsibilities

## Problem

`GadgetHost` owns five conceptually independent responsibilities
behind a single struct:

1. **Gadget registry** — `slots: Vec<GadgetSlot>`, registration,
   source-kind mapping.
2. **Search routing** — prefix matching, catalog fuzzy matching with
   nucleo, concurrent query dispatch, result merging.
3. **Shortcut management** — global shortcut registration, reactive
   re-registration, `watched_keys` set.
4. **Settings-change dispatch** — routing `enabled.<id>` and
   `gadgets.<id>.*` changes, driving enable/disable transitions.
5. **Entry store correlation** — caching search results so
   `execute()` can pass the full `ScoredEntry` to the gadget.

## Partially addressed

The capability refactor resolved the FIXME about AppHandle/
GadgetContext on `enable()`: gadgets now receive context through
`ProvisioningContext` at provision time, and `GadgetContext` is
removed. `AppHandle` no longer appears in the `Gadget` trait.

## Remaining work

The core decomposition is untouched:
- Extract `ShortcutManager`, `SearchRouter`, and
  `SettingsDispatcher` as distinct types.
- `GadgetHost` becomes a thin coordinator that owns the slot
  registry and delegates.
- The shortcut system gets its own lifecycle
  (`shortcut_signal_rx: Mutex<Option<Receiver>>` pattern).
- Search routing and shortcut management share no state except
  `self.slots` and `self.store`, but are forced into the same
  struct.

## Affected files

- `src-tauri/src/gadget_host.rs`
- `src-tauri/src/lib.rs` (settings-changed listener, initialization)
