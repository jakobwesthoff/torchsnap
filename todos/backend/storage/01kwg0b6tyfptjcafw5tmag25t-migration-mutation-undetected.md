---
kind: bug
severity: medium
status: open
area: [src-tauri/src/storage/sql_storage.rs]
tags: [unconfirmed]
---

# Edited SQL migrations are silently never re-applied

## Problem
`SqlStorage::open` applies migrations via `rusqlite_migration`
(`src-tauri/src/storage/sql_storage.rs:290-295`), which tracks
progress solely through SQLite's `user_version` counter — it
stores no checksum of applied migration text. Consequently,
editing an already-applied migration file changes nothing for
existing databases: the migration index is unchanged, so the
edited SQL is never executed, and no error or warning surfaces.

This matters most for gadget authors: `[storage.sql]`
migrations are plain files in the gadget
(`manifest/storage.rs`), and during development the natural
mistake is to edit `001_init.sql` instead of adding
`002_change.sql`. The dev database (under
`<app_data_dir>/gadget-home/<id>/sql/`) keeps the old schema
while a fresh install gets the new one, producing
works-on-fresh-install-only bugs with no diagnostic.

## Impact
Silent schema divergence between existing and fresh databases,
for both host-internal stores and gadget databases. Debugging
cost lands on gadget authors who have no visibility into the
mechanism.

## Suggested fix
Store a hash of each applied migration (a tiny
`_migrations(hash)` side table or a blake3 of the concatenated
prefix kept in gadget-home) and, on mismatch, fail gadget enable
with "migration N changed after being applied — add a new
migration instead". At minimum, document the append-only rule
prominently in the gadget SDK storage docs.
