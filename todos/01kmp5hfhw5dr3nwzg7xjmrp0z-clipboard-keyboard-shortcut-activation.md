# Clipboard Plugin: Keyboard Shortcut Activation

Allow opening the clipboard manager's custom UI directly via a global
keyboard shortcut, without first opening the launcher and navigating
to it through the result list.

## Scope

- Register a configurable global shortcut (e.g., Cmd+Shift+V) that
  opens the launcher directly into the clipboard history view
- Should reuse the same custom UI component as the list-entry path
- Needs to integrate with the existing global shortcut system
  (`tauri-plugin-global-shortcut`)

## Dependencies

- Clipboard manager plugin must be implemented first
- May depend on the keybind system todo for a unified shortcut
  management approach

## Open questions

- Should this be a general capability (any plugin can register a
  direct-activation shortcut) or clipboard-specific?
- If general: does this belong in the plugin trait as an optional
  method (e.g., `fn global_shortcuts(&self) -> Vec<Shortcut>`)?
- Interaction with the existing launcher toggle shortcut — does ESC
  dismiss the clipboard view and return to normal launcher, or
  dismiss entirely?
