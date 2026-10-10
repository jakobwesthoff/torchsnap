---
title: "Update the settings reactivity doc's stale module paths"
kind: docs
component: backend
impact: low
---
# Update the settings reactivity doc's stale module paths

`docs/Gadget-Architecture/06-settings-reactivity.md` still cites the pre-refactor single-file settings layout in two places, although the settings code now lives in the `src-tauri/src/settings/` module directory.

## Scope

The settings code moved into a module directory
(`src-tauri/src/settings/` containing `mod.rs`, `notifier.rs`,
`coalescing_dispatcher.rs`), but two references in the doc still
point at the old single-file layout:

- `06-settings-reactivity.md:251` says "`src-tauri/src/settings_notifier.rs`
  keeps `SettingsWatch<T>`". Actual location:
  `src-tauri/src/settings/notifier.rs`.
- `06-settings-reactivity.md:170-171` says "`GadgetSettings`
  (`src-tauri/src/settings.rs`)". Actual location:
  `src-tauri/src/settings/mod.rs`.

The `coalescing_dispatcher.rs` reference (`:107`) already uses the
module path and is correct. All behavioral claims in the document
checked out against the code (key layout, three-path fan-out,
dispatcher dedup semantics, `GadgetShortcut` fields at
`src-tauri/src/gadgets/mod.rs:37-42`, `open-gadget-settings` emit
at `src-tauri/src/gadget_host.rs:971`).

## Done when

- Both stale paths in the document name the files that hold the code.
