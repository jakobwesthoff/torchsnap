---
title: "Surface actionable backend errors in the frontend"
kind: feature
component: frontend
status: blocked
---
# Surface actionable backend errors in the frontend

Failed searches and every other command rejection that no caller handles reach the frontend only as `console.warn` output. The typed `command()` wrapper in `src/lib/command.ts` logs every rejection before rethrowing, and `useSearch` (`src/launcher/hooks/useSearch.ts`) neither catches nor shows a failed `search` command. Nothing reaches the user, so a broken search stays invisible in a release build.

ADR 0062 already covers failed launcher actions: the footer shows a failed `search_execute` and failures of `onExecute` and `openSettings()` from views (`useActionError` in `src/launcher/hooks/useActionError.ts`, `showError` in `useLauncher()`). `ClipboardView` reports failed paste and delete through `showError`. This todo covers what remains.

## What's needed

- Decide where a failed search shows. The footer line of ADR 0062 may be enough, or the result area needs its own state.
- Decide for the generic `command()` wrapper which rejections are actionable (the user can retry) and which are internal (just log). Surface only the actionable ones, through the footer error line or a new notification, instead of leaving them at `console.warn`.

## Affected call sites

- `command("search", ...)` in `useSearch`: search failures
- any other `command(...)` caller without its own catch, found by searching `command(` in `src`

## Relations

- Depends on: [Return structured error types from Tauri commands](01KN44HD21R2SV9Y457DR9C11M-return-structured-error-types-from-tauri-commands.md)
