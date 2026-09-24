---
kind: improvement
severity: low
status: open
area: [src/lib/command.ts, src/types.ts, src/launcher/compareEntries.ts]
---

# Frontend comments reference the nonexistent `src-tauri/src/search/` module

## Problem

Several frontend sync-obligation comments point at a module path
that no longer exists:

- `src/lib/command.ts:16-18`:

  > The `CommandMap` must stay in sync with the `generate_handler!`
  > registration and `#[tauri::command]` signatures in
  > `src-tauri/src/lib.rs` and `src-tauri/src/search/mod.rs`.

- `src/types.ts` — five section headers cite the dead path:
  lines 18, 57, 95 ("Mirrors `src-tauri/src/search/types.rs`"),
  line 45 ("Mirrors `PostAction` in
  `src-tauri/src/search/types.rs`"), and line 108 ("Mirrors
  `SearchMessage` in `src-tauri/src/search/mod.rs`").

- `src/launcher/compareEntries.ts:11-12`: "The Rust backend has
  an equivalent method `SourcedEntry::cmp_sort_key` in
  `src-tauri/src/search/types.rs` that MUST stay in sync". This
  one is the most load-bearing of the set — it states a
  hard sync obligation on the sort comparator (the Rust side
  points back correctly from
  `src-tauri/src/commands/types.rs:201-204`).

There is no `src-tauri/src/search/` directory in the tree. The
search/gadget command handlers live in
`src-tauri/src/commands/mod.rs` (e.g. `search_execute`,
`gadget_message`) with shared types in
`src-tauri/src/commands/types.rs`.

Since these comments state the sync obligations that keep the
typed `CommandMap` and the `PostAction` mirror honest, pointing
them at a dead path sends the next maintainer grepping in the
wrong place.

## Suggested fix

Point both comments at `src-tauri/src/commands/mod.rs` /
`src-tauri/src/commands/types.rs` (the `src-tauri/src/lib.rs`
half of the command.ts comment is still correct).
