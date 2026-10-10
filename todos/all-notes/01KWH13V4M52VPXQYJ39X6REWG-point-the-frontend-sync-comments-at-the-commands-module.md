---
title: "Point the frontend sync comments at the commands module"
kind: chore
component: frontend
impact: low
tags: [ux]
---
# Point the frontend sync comments at the commands module

Several frontend sync-obligation comments point at `src-tauri/src/search/`, a module path that no longer exists. These comments state the obligations that keep the typed `CommandMap` and the `PostAction`, entry and message mirrors in sync. Pointing them at a dead path sends the next maintainer grepping in the wrong place.

The `src/types.ts` "Action Types" section was already fixed as a side effect of the entry-action-commands rewrite. It now says "Mirrors `src-tauri/src/commands/types.rs`" (`src/types.ts:18`).

## Problem

The remaining stale references:

- `src/lib/command.ts:16-18`:

  > The `CommandMap` must stay in sync with the `generate_handler!`
  > registration and `#[tauri::command]` signatures in
  > `src-tauri/src/lib.rs` and `src-tauri/src/search/mod.rs`.

- `src/types.ts`: four remaining section headers cite the dead path: line 39 ("Mirrors `PostAction` in `src-tauri/src/search/types.rs`"), line 51 ("Mirrors `src-tauri/src/search/types.rs`", Entry Types), line 89 (same, Gadget View Reference), and line 102 ("Mirrors `SearchMessage` in `src-tauri/src/search/mod.rs`").

- `src/launcher/compareEntries.ts:11-12`: "The Rust backend has an equivalent method `SourcedEntry::cmp_sort_key` in `src-tauri/src/search/types.rs` that MUST stay in sync". This is the most load-bearing of the set, because it states a hard sync obligation on the sort comparator. The Rust side points back correctly from `src-tauri/src/commands/types.rs:201-204`.

There is no `src-tauri/src/search/` directory in the tree. The search and gadget command handlers live in `src-tauri/src/commands/mod.rs` (for example `search_execute` and `gadget_message`), with shared types in `src-tauri/src/commands/types.rs`.

## Suggested fix

Point the remaining comments at `src-tauri/src/commands/mod.rs` and `src-tauri/src/commands/types.rs`. The `src-tauri/src/lib.rs` half of the `command.ts` comment is still correct.
