# Gadget `execute()` errors never surface in the launcher UI

**Kind:** bug
**Severity:** medium
**Area:** src/launcher/Launcher.tsx

## Problem

When a gadget's `execute()` returns `Err(string)`, the value
propagates as a rejected `search_execute` invoke. The frontend
executor swallows it:

- `executeEntry` (`src/launcher/Launcher.tsx:549-576`) does
  `await command("search_execute", …)` with no `try`/`catch`;
  the enclosing `useCallback` is `async`, so the rejection
  becomes an unhandled promise rejection.
- The `command` wrapper (`src/lib/command.ts:186-192`) only
  `console.warn`s and re-throws.
- There is no toast/banner component anywhere under `src/`
  (`grep -ri toast src/` is empty), so nothing is shown to the
  user.

Net effect: the user presses Enter on a result, the action
fails, and the launcher gives zero feedback — the window stays
open with nothing happening (or the palette closes as if it
succeeded, depending on timing). The gadget author's error
string (which `docs/api/gadget-development.md:388-390` claims is
shown "as a toast in the launcher ... verbatim") is only
visible in the devtools console.

Same gap applies to the inline-execute path
(`handleInlineExecute` feeding `Launcher.tsx:540`) since it goes
through the same un-caught `command` call.

## Impact

Every failing gadget action is a silent no-op for the user.
Gadgets that rely on returning `Err` to signal recoverable
problems ("API token missing", "network unreachable") have no
user-visible channel at all.

## Suggested fix

Catch rejections in `executeEntry`, and surface the message in
the launcher (footer status line, inline banner, or an actual
toast layer). Whichever surface is chosen, update
`docs/api/gadget-development.md` (toast claim at :388-390) to
match. A regression test that a failing execute leaves visible
feedback would pin the contract.
