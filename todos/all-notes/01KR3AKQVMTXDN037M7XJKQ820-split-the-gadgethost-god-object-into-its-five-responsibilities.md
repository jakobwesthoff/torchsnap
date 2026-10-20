---
title: "Split the GadgetHost god object into its five responsibilities"
kind: refactor
component: gadget-host
---
# Split the GadgetHost god object into its five responsibilities

`GadgetHost` in `src-tauri/src/gadget_host.rs` holds five conceptually independent responsibilities in one struct. The capability refactor removed the AppHandle/GadgetContext FIXME from `enable()`, but the decomposition itself has not started.

## Problem

`GadgetHost` owns five conceptually independent responsibilities
behind a single struct:

1. **Gadget registry**: `slots: Vec<GadgetSlot>`, registration,
   source-kind mapping.
2. **Search routing**: prefix matching, catalog fuzzy matching with
   nucleo, concurrent query dispatch, result merging.
3. **Shortcut management**: global shortcut registration, reactive
   re-registration, `watched_keys` set.
4. **Settings-change dispatch**: routing `enabled.<id>` and
   `gadgets.<id>.*` changes, driving enable/disable transitions.
5. **Entry store correlation**: caching search results so
   `execute()` can pass the full `ScoredEntry` to the gadget.

## Partially addressed

The capability refactor resolved the FIXME about AppHandle/
GadgetContext on `enable()`: gadgets now receive context through
`ProvisioningContext` at provision time, and `GadgetContext` is
removed. `AppHandle` no longer appears in the `Gadget` trait.

## Remaining work

The core decomposition has barely started. Some pieces of item 4 moved
since: `GadgetSlot::submit` owns each gadget's change queue,
`route_setting_key` and `deliver_setting_change` route a settings key to
its slot, and `EnableFailures` lives in `src-tauri/src/gadget_status.rs`.
These are free functions and a helper type, not the extraction below.
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
