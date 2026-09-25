---
kind: feature
status: open
---

# Handle backend errors in the frontend

The typed `command()` wrapper (`src/lib/command.ts:245-253`) now logs
every rejection with `console.warn` before rethrowing, so failures are
visible in development. `sendMessage` routes through the
`gadget_message` command (`src/lib/gadgetMessage.ts:31`), so it gets
that logging too. Nothing surfaces to the user: there is still no
toast or inline error banner, so failed clipboard pastes, broken
searches, and gadget message errors remain invisible in a release
build.

## What's needed

- A lightweight error notification system (e.g., transient toast or
  inline error banner in the launcher).
- Route the errors `command()` already catches into that notification
  system for actionable cases, instead of leaving them at
  `console.warn`.
- Consider which errors are actionable (user can retry) vs. internal
  (just log), and only surface actionable ones.

## Affected call sites

- `sendMessage("paste", ...)` (`src/gadgets/clipboard/ClipboardView.tsx:338`): clipboard write failure
- `sendMessage("delete", ...)` (`src/gadgets/clipboard/ClipboardView.tsx:345`): entry deletion failure
- `command("search_execute", ...)`: action execution failure
- `command("search", ...)`: search failures
