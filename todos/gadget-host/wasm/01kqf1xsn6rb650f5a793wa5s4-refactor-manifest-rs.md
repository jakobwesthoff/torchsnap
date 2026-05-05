# Refactor `src-tauri/src/wasm/manifest.rs`

**Status: TODO — file has grown past a maintainable size.**

## Problem

`src-tauri/src/wasm/manifest.rs` is currently ~2700 lines covering:

- The TOML schema (`Manifest`, `PluginDef`, `SettingsDef`,
  `StorageDef`, `FrontendDef`, `TaskDef`, `PermissionsDef` and all
  its sub-structs).
- The full `[[permissions.command]]` argv vocabulary
  (`CommandPermissionDef`, `ArgvConstraint`, every kind variant).
- Parse-time validation for every permission shape — opener, http,
  fs, command argv shapes, command rule overlap detection.
- A large, varied test suite covering every error path.

Adding a new permission shape (`fs` was the most recent example)
requires hunting through several thousand lines to find the right
insertion points: the struct definition, the `PermissionsDef`
field, the `validate_permissions` step, any inline tests. The
edit-amplification factor is high and reviewers can't easily see
what changed.

## Proposed shape

Split into a `manifest/` directory module with one file per
concern. Rough sketch — refine when the work begins:

```
src-tauri/src/wasm/manifest/
├── mod.rs              -- top-level Manifest, public API
├── plugin.rs           -- PluginDef, FrontendDef, TaskDef
├── settings.rs         -- SettingsDef
├── storage.rs          -- StorageDef
├── permissions/
│   ├── mod.rs          -- PermissionsDef, the dispatcher
│   ├── opener.rs       -- OpenerPermissionsDef + validation
│   ├── http.rs         -- HttpPermissionsDef + validation
│   ├── fs.rs           -- FsPermissionsDef + validation
│   └── command/
│       ├── mod.rs      -- CommandPermissionDef + dispatcher
│       ├── argv.rs     -- ArgvConstraint variants + validation
│       └── overlap.rs  -- detect_command_rule_overlap
└── tests/              -- one test file per concern, parallel structure
```

Tests follow the same split — each module owns its own
`#[cfg(test)] mod tests` block (or a sibling `tests.rs`).

## Why this hasn't happened yet

The file grew organically as new permissions landed (opener →
http → website-metadata → command rules → fs). Each addition was
small enough to land in-place; the cumulative weight only became
obvious after the fs work added another ~150 lines.

## Constraints when refactoring

- **Public API stable.** External callers import `Manifest`,
  `PermissionsDef`, the various `*PermissionsDef` types. Re-export
  these from `manifest/mod.rs` so callers don't need to update
  their imports.
- **Test coverage parity.** Every existing test must move with
  its target type. No tests deleted in the refactor; coverage
  stays at the current level or grows.
- **No behaviour change.** This is purely a file-organization
  refactor. Any deserialization shape change, validation
  tightening, or error-message rewording is out of scope; do it
  in a separate PR before or after.
- **One PR.** A staged refactor where the file lives in two
  places at once invites silent drift. Land the split as one
  diff, even if large.

## When to do this

- Before the next permission shape is added (so that addition
  lands in a clean structure).
- Or: when the next bug requires touching multiple permission
  validators and the navigation cost becomes painful.

Whichever comes first. Not blocking on the ZeroTier plugin work.
