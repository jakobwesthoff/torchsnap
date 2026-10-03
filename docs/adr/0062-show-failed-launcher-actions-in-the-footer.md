# 62. Show failed launcher actions in the footer

Date: 2026-10-03

## Status

Accepted

Amends [13. Allow plugins to provide custom UI components for the result area](0013-allow-plugins-to-provide-custom-ui-components-for-the-result-area.md) (`onExecute` returns a promise)

Amends [28. Plugin component contract via context and grouped hooks](0028-plugin-component-contract-via-context-and-grouped-hooks.md) (`useLauncher()` gains `showError`)

Amends [55. Dispatch entry actions through gadget commands in fixed slots](0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md) (failures of `onExecute` and `openSettings()` show in the footer)

## Context

When a gadget's `execute()` failed, the launcher showed nothing. The
list path awaited the `search_execute` command without catching its
rejection, and the `command` wrapper only writes a console warning
before passing the rejection on. The launcher had no place to show an
error.

Views run entry actions with `onExecute(entryId, slot)`, which ADR 13
declared as returning `void`, so a view could not learn that an action
failed. Views that act through `sendMessage` handled failures
themselves: the calculator logged a failed copy, and the clipboard
view left a failed paste or delete as an unhandled rejection.

## Decision

While a launcher action has failed, the footer shows one error line in
place of its hints. The line is cut to one line and carries the full
text as tooltip. It reads `<lead> failed: <message>`:

- for an entry in the result list, the lead is the entry's title,
- for `onExecute` from a custom or inline view, the lead is the
  gadget's name,
- for `openSettings()` from a view, the lead is
  `Opening <gadget name> settings`.

The error goes away on the next action, on a key press other than a
lone modifier, when the query, the selection or the custom view
changes, and when the launcher hides.

`onExecute` returns `Promise<void>`. It rejects with the error message
when the action fails. The launcher attaches its own handler to the
promise before returning it, so a view may ignore the promise.
`openSettings()` keeps rejecting when the Settings window cannot open,
and its failure shows in the same footer line.

`useLauncher()` gains `showError(message)`. A view reports a failure
of its own action, such as a `sendMessage` call, with it; the footer
shows the message as given, with the same clearing rules. The
calculator's copy and the clipboard view's paste and delete use it.

The gadget API version stays at 0.2.0. The WIT and the Rust SDK do
not change. In the TypeScript SDK, `MockGadgetContextProvider` accepts
an `onExecute` override that returns nothing and wraps it in a
promise.

## Consequences

The launcher's footer can show either the hints or an error, never
both. A view that awaits `onExecute` sees the rejection in addition to
the footer line.
