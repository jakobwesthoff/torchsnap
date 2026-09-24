---
kind: improvement
severity: low
status: open
area: [docs/api/gadget-development.md]
tags: [docs]
---

# gadget-development.md: stale line/file references and minor type inaccuracies

## Problem

Cross-checking the guide against the current tree (2026-07-02)
found a batch of references that have drifted. None affect the
conceptual content; all mislead a reader who follows them.

### Stale WIT line references (file is now 927 lines; all shifted ~20)

| Doc claim | Actual location in `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` |
|-----------|------------------------------------------------------------------|
| world `gadget` "lines 887–907" (`gadget-development.md:39-40,643`) | 907–927 |
| `SearchResponse` "WIT lines 733–745" (`:274`) | 753–765 |
| `ActionId` "WIT 678–686" (`:347`) | 694–702 |
| `Action` "WIT 688–691" (`:342`) | 704–707 |
| `handle-message` "lines 790–793" (`:396-397`) | 810–813 |
| clipboard "WIT lines 35–55" (`:670`) | interface at 50–59 |

### Obsolete stale-comment callout

`gadget-development.md:705-711` block-quotes that the WIT
`interface sql` doc-comment (cited as `torchsnap-gadget.wit:60-61`)
still claims the database lives at
`app_data_dir/gadgets/<gadget-id>/storage.db` and labels it
stale. The WIT has since been fixed — `torchsnap-gadget.wit:63-64`
now states the correct
`<app_data_dir>/gadget-home/<gadget-id>/sql/storage.sqlite3`
path — so the callout describes a problem that no longer exists.

### Stale file references

- "`The full schema is in src-tauri/src/wasm/manifest.rs`"
  (`gadget-development.md:491`), plus pinpoint refs
  `manifest.rs:171` (`:507`), `manifest.rs:570–610` (`:609`),
  `manifest.rs:54` (`:617`). The manifest parser is now the
  module directory `src-tauri/src/wasm/manifest/` (`mod.rs`,
  `frontend.rs`, `permissions/*.rs`, `storage.rs`, `tasks.rs`,
  `paths.rs`); the single-file paths and line numbers are dead.
  (Shortcuts table: now `manifest/mod.rs:68-72`; id validation
  lives in `manifest/mod.rs`.)
- `packages/gadget-sdk/src/shims/hooks.ts:88` for the
  onMessage-drop rationale (`gadget-development.md:429,1047`);
  the comment now sits at `hooks.ts:93-116`.

### World import list is wrong

`gadget-development.md:643-649` says the world "imports fourteen
interfaces" and lists `messaging` among them. `messaging` is a
guest **export** (`torchsnap-gadget.wit:925`); the fourteenth
import is the shared `types` interface (`torchsnap-gadget.wit:920`),
which the list omits. The list also uses the SDK module aliases
(`sql`, `fs`, `paths`) for WIT interfaces named `sql-storage`,
`filesystem`, `path-resolver` without noting the mapping.

### Minor type inaccuracies

- `clipboard::write_text` is documented as returning
  `Result<(), String>` (`gadget-development.md:667`); the WIT
  returns `result<_, clipboard-error>` with a
  `backend-failure(string)` variant (`torchsnap-gadget.wit:52-58`).
- "Returning `Err(string)` surfaces in the host log and **as a
  toast in the launcher**" (`gadget-development.md:388-390`) —
  no toast UI exists in `src/` (see companion frontend todo
  `frontend/01kwh13v4m52vpxqyj39x6rewf-execute-errors-invisible-in-launcher.md`);
  today the error is only visible in devtools/console.

## Suggested fix

Replace hard line numbers with anchor-style references (interface
names, section titles) that survive WIT edits; fix the file paths
to the manifest module; correct the import list; drop or reword
the obsolete sql-comment callout; align the clipboard error type;
reword the execute-error claim to match actual behavior (or keep
the claim and implement the toast — see the frontend todo).
