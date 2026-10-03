---
kind: question
severity: low
status: needs-discussion
area: [src-tauri/src/wasm/manifest/mod.rs, src-tauri/src/wasm/manifest/permissions/mod.rs]
---

# Decide whether `[storage.sql]` without `sql-storage = true` is legal

`Manifest::parse` rejects `permissions.sql-storage = true` without a
`[storage.sql]` table. The reverse is still accepted unchecked: a
manifest with `[storage.sql]` and its migrations but no
`sql-storage = true` parses. The bridge provisions SQL storage only
when both are present (`src-tauri/src/wasm/bridge.rs`, "SqlStorage"
block), so such a gadget gets no database.

## Open question

Is that manifest legal, declaring storage the gadget never gets, or a
parse error naming both keys, like the forward case?

## Once decided

- Parse error: add the check next to the forward one in
  `Manifest::parse`, with a parse test, and state it in the manifest
  docs (`torchsnap-docs`, `development/manifest.mdx`, `[storage.sql]`
  section).
- Legal: document that `[storage.sql]` without the permission
  provisions no database.
