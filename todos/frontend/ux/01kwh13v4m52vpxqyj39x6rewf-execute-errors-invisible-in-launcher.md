---
kind: bug
severity: medium
status: open
area: [src/launcher/Launcher.tsx]
---

# Gadget `execute()` errors never surface in the launcher UI

## Problem

When a gadget's `execute()` returns `Err(string)`, the value
propagates as a rejected `search_execute` invoke. The frontend
executor swallows it:

- `executeEntry` (`src/launcher/Launcher.tsx:600-621`) does
  `await command("search_execute", …)` with no `try`/`catch`;
  the enclosing `useCallback` is `async`, so the rejection
  becomes an unhandled promise rejection.
- The `command` wrapper (`src/lib/command.ts:251-252`) only
  `console.warn`s and re-throws.
- There is no toast/banner component anywhere under `src/`
  (`grep -ri toast src/` is empty), so nothing is shown to the
  user.

Net effect: the user presses Enter on a result, the action
fails, and the launcher gives zero feedback — the window stays
open with nothing happening (or the palette closes as if it
succeeded, depending on timing). The error string is only
visible in the devtools console. (`docs/api/gadget-development.md`
no longer claims a toast; it now says only that `Err(string)`
"rejects the launcher's `search_execute` call".)

Same gap applies to the inline-execute path
(`handleInlineExecute`, `Launcher.tsx:409`) since it goes
through the same un-caught `command` call.

## Impact

Every failing gadget action is a silent no-op for the user.
Gadgets that rely on returning `Err` to signal recoverable
problems ("API token missing", "network unreachable") have no
user-visible channel at all.

## Suggested fix

Catch rejections in `executeEntry`, and surface the message in
the launcher (footer status line, inline banner, or an actual
toast layer). A regression test that a failing execute leaves
visible feedback would pin the contract.
