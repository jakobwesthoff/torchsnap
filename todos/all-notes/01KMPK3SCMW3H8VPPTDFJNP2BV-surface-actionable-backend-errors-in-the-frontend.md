---
title: "Surface actionable backend errors in the frontend"
kind: feature
component: frontend
status: blocked
---
# Surface actionable backend errors in the frontend

Backend command errors reach the frontend only as `console.warn` output. The typed `command()` wrapper in `src/lib/command.ts` (around lines 245-253) logs every rejection before rethrowing, and `sendMessage` routes through the `gadget_message` command (`src/lib/gadgetMessage.ts`), so it gets that logging too. Nothing reaches the user. There is still no toast or inline error banner, so failed clipboard pastes, broken searches and gadget message errors stay invisible in a release build.

## What's needed

- A lightweight error notification system (for example a transient toast or an inline error banner in the launcher).
- Route the errors that `command()` already catches into that notification system for actionable cases, instead of leaving them at `console.warn`.
- Decide which errors are actionable (the user can retry) and which are internal (just log). Surface only the actionable ones.

## Affected call sites

- `sendMessage("paste", ...)` (`src/gadgets/clipboard/ClipboardView.tsx`): clipboard write failure
- `sendMessage("delete", ...)` (`src/gadgets/clipboard/ClipboardView.tsx`): entry deletion failure
- `command("search_execute", ...)`: action execution failure
- `command("search", ...)`: search failures

## Relations

- Depends on: [Return structured error types from Tauri commands](01KN44HD21R2SV9Y457DR9C11M-return-structured-error-types-from-tauri-commands.md)
