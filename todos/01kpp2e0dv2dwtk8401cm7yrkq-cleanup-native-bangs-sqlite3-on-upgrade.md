# Clean up orphaned native `bangs.sqlite3` on upgrade

## Context

When the bangs plugin was native (pre WASM conversion), its SQL storage
lived at `<app_data_dir>/plugin-home/bangs/sql/bangs.sqlite3`. The
filename was chosen by the native plugin itself (see the pre-removal
`src-tauri/src/plugins/bangs/mod.rs:126`: `data_dir.join("sql").join("bangs.sqlite3")`).

The WASM bridge uses a different convention —
`<app_data_dir>/plugin-home/<id>/sql/storage.sqlite3` — applied uniformly
to every WASM plugin with a `[storage.sql]` block. After the WASM
conversion landed (commit `ab67644`), existing users upgrading from a
version that shipped the native plugin end up with:

- `plugin-home/bangs/sql/bangs.sqlite3` — ~2–5 MB SQLite from the old
  native plugin, orphaned. Never read again. Never cleaned up.
- `plugin-home/bangs/sql/storage.sqlite3` — the new WASM plugin's
  fresh database, populated from the bundled `assets/bang.json` on
  first enable.

Data loss is cosmetic (the `metadata.source` = "network" and the
original `import_date` from the user's last refresh get silently
replaced by a fresh `"builtin"` import), but the orphaned file sits
forever unless something explicitly cleans it up.

## What to do

Add a one-shot migration hook that, on app startup, deletes the legacy
`<app_data_dir>/plugin-home/bangs/sql/bangs.sqlite3` if present. Log
the removal at info level so a concerned user can see it happened.

Candidate locations for the cleanup code:

- A small `plugin_install`-level migration function invoked early in
  `lib.rs` startup, alongside the WASM plugin discovery.
- A bangs-specific cleanup step — but this pulls plugin-id knowledge
  into the host, which feels wrong for a one-off migration.

Prefer the first. Idempotent: if the file doesn't exist, no-op.

## Acceptance

- Fresh install: no change (file never existed).
- Upgrade from native: on first launch, `bangs.sqlite3` is removed;
  a log entry records the removal.
- Second launch: no-op.

## Related

- Commit `ab67644` — bangs: remove native plugin and derived bang.json.
- Commit `31ffa01` — plugins/bangs: port guest to WASM.
- Code review on 2026-04-20 flagged this as an "Issue" (review agent's
  classification, non-blocking).
