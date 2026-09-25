---
kind: bug
severity: high
status: open
area: [gadgets/calculator/src/lib.rs, gadgets/calculator/frontend/src/views/CalculatorInline.tsx, gadgets/calculator/frontend/src/views/CalculatorView.tsx, src-tauri/src/gadget_host.rs]
tags: [wasm]
---

# Calculator copy on Enter does nothing

## Problem

Pressing Enter on a calculator result does not copy it to the
clipboard, and the launcher stays open. The maintainer confirmed this
in the running app. The history entry is still saved.

This affects every place the calculator frontend copies a result:

- the inline result in heuristic mode
  (`gadgets/calculator/frontend/src/views/CalculatorInline.tsx:58`),
- Enter in prefix mode (`=`), for the current result or the selected
  history row (`gadgets/calculator/frontend/src/views/CalculatorView.tsx:172`),
- clicking a history row (`CalculatorView.tsx:274`).

## Cause

The views call `onExecute(resultString, { type: "copy" })` and pass
the computed result as the entry id. The gadget's `execute()`
(`gadgets/calculator/src/lib.rs:145`) was written to copy `entry.id`
to the clipboard.

Since fd86a52 ("Change Gadget::execute() to receive full ScoredEntry",
2026-05-07), `GadgetHost::execute` first looks the entry up in the
entry store by `(gadget, entry id)`. When the lookup fails, it logs
"not found in entry store" and returns `PostAction::Nothing`
(`src-tauri/src/gadget_host.rs:1010`). A result string is never a
stored entry id: the inline response has `results: vec![]`
(`gadgets/calculator/src/lib.rs:235`), and history rows use their
database id as the entry id (`gadgets/calculator/src/lib.rs:636`). So
`execute()` is never called, nothing is copied, and `Nothing` keeps the
launcher open.

`save_history` still works because the views send it separately
through `sendMessage` before calling `onExecute`.

## Suggested fix

Test-first: a regression test that shows the copy path failing, then
the fix.

A fix that needs no API change: add a calculator message, for example
`copy`, whose handler writes the result to the clipboard with
`clipboard::write_text` and saves the history entry. The views call it
and then `dismiss()`, which `LauncherActions` already provides
(`packages/gadget-sdk/src/shims/hooks.ts:126`). This also removes the
current fire-and-forget `save_history` call that races the dismiss.

This fix follows the rule for views in ADR 55
(`docs/adr/0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md`):
a view that acts on its own state calls its backend with `sendMessage`
and then a launcher action. Fix the bug before that redesign
(`todos/gadget-host/api/01kr2357mcz36g4gte0c0t1qz5-entry-action-commands-and-slots.md`)
lands. The redesign later moves history rows to entry commands run
with `onExecute(entryId, slot)`.
