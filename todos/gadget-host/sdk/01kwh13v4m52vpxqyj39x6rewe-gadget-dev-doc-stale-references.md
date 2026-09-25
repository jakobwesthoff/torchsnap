---
kind: improvement
severity: low
status: open
area: [docs/api/gadget-development.md]
tags: [docs]
---

# gadget-development.md: stale line/file references and minor type inaccuracies

## Problem

Re-checked against the tree after the ADR 55/56 rewrite
(`docs/api/gadget-development.md` grew to 1482 lines). The
`ActionId`/`execute(entry_id, action_id)` items this todo
originally listed are gone along with the type. No action
needed there. What's left:

### Stale WIT line references

| Doc claim | Actual location in `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` |
|-----------|------------------------------------------------------------------|
| world `gadget` "lines 887–907" (`gadget-development.md:47`, `:798`) | 935–955 |
| clipboard "WIT lines 35–55" (`gadget-development.md:825`) | interface at 54–62 |

### Obsolete stale-comment callout

`gadget-development.md:860-865` block-quotes that the WIT
`interface sql` doc-comment (cited as `torchsnap-gadget.wit:60-61`)
still claims the database lives at
`app_data_dir/gadgets/<gadget-id>/storage.db` and labels it
stale. The WIT comment above `interface sql-storage`
(`torchsnap-gadget.wit:65-85`) has since been rewritten and never
mentions `storage.db`: it already states the correct
`<app_data_dir>/gadget-home/<gadget-id>/sql/storage.sqlite3` path.
The callout now describes a problem that doesn't exist; delete it.

### Stale file references

- "`The full schema is in src-tauri/src/wasm/manifest.rs`"
  (`gadget-development.md:646`), plus pinpoint refs
  `manifest.rs:171` (`:662`), `manifest.rs:570–610` (`:764`),
  `manifest.rs:54` (`:772`). The manifest parser is now the
  module directory `src-tauri/src/wasm/manifest/` (`mod.rs`,
  `frontend.rs`, `permissions/*.rs`, `storage.rs`, `tasks.rs`,
  `paths.rs`). Corrected targets: id validation is
  `validate_gadget_id` at `manifest/mod.rs:185`; the
  argv-constraint vocabulary is documented at
  `manifest/permissions/command.rs:145-183`; the `[shortcuts]`
  field is `manifest/mod.rs:68-72`.

### World import list is wrong

`gadget-development.md:798-804` still says the world "imports
fourteen interfaces" and lists `messaging` among them.
`messaging` is a guest **export**
(`torchsnap-gadget.wit:953`, inside `world gadget { ... export
messaging; ... }`); the fourteenth import is the shared `types`
interface (`torchsnap-gadget.wit:948`), which the list omits.

### Minor type inaccuracy

- `clipboard::write_text` is documented as returning
  `Result<(), String>` (`gadget-development.md:822`); the WIT
  returns `result<_, clipboard-error>` with a
  `backend-failure(string)` variant
  (`torchsnap-gadget.wit:56-61`).

## Suggested fix

Replace hard line numbers with anchor-style references (interface
names, section titles) that survive WIT edits; fix the file paths
to the manifest module using the targets above; correct the
import list (drop `messaging`, add `types`); delete the obsolete
sql-comment callout; align the clipboard error type.
