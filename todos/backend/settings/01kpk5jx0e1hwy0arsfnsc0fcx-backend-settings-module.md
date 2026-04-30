# Backend: consolidate settings into a module (`src-tauri/src/settings/`)

Minor backend cleanup: the settings subsystem currently lives as three
sibling files at the root of `src-tauri/src/`. Move them into a `settings/`
module for consistency with how other subsystems (search, frecency, icons,
network, storage, wasm) are organized.

## Why

`src-tauri/src/` is already well-organized vertically. This is a small
consistency fix — the settings subsystem is the only multi-file domain that
hasn't graduated to a module directory yet.

## Files to move

| Current path | Target path |
|---|---|
| `src-tauri/src/settings.rs` | `src-tauri/src/settings/mod.rs` |
| `src-tauri/src/settings_notifier.rs` | `src-tauri/src/settings/notifier.rs` |
| `src-tauri/src/coalescing_dispatcher.rs` | `src-tauri/src/settings/coalescing_dispatcher.rs` |

Re-export from `settings/mod.rs` so call sites in `lib.rs` don't need to
change their `use` paths (e.g. `use crate::settings::SettingsStore` stays valid).

## Optional: move plugin host/install into `plugins/`

Also consider moving:
- `src-tauri/src/plugin_host.rs` → `src-tauri/src/plugins/host.rs`
- `src-tauri/src/plugin_install.rs` → `src-tauri/src/plugins/install.rs`

These are already conceptually part of the `plugins/` subsystem. Lower
priority — the current placement works fine, this is purely cosmetic.

## Priority: LOW (cosmetic consistency, small blast radius)
