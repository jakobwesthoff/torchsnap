---
kind: bug
severity: low
status: open
area: [src/components/ShortcutRecorder.tsx, src/keybindings/accelerator.ts]
tags: [macos]
---

# ShortcutRecorder records Ctrl as Cmd on macOS

## Problem

`buildAccelerator` (`src/components/ShortcutRecorder.tsx`) maps both
modifier flags to one token:

```ts
if (e.metaKey || e.ctrlKey) {
  parts.push("CommandOrControl");
}
```

In a Tauri accelerator, `CommandOrControl` means Cmd on macOS. So on
macOS:

- pressing Ctrl+Space records `CommandOrControl+Space`, which
  registers and displays as Cmd+Space;
- pressing Cmd+Ctrl+K records `CommandOrControl+K` and drops Ctrl.

The recording hint names ⌘, ⌃ and ⌥ as accepted modifiers on macOS,
and Ctrl is accepted, but the saved shortcut is a different one from
what was pressed.

`accelToDisplayParts` (`src/keybindings/accelerator.ts`) only knows
`CommandOrControl`/`CmdOrCtrl`, `Shift` and `Alt`; any other token is
shown as a key name.

## Suggested fix

On macOS, record Cmd and Ctrl as separate tokens (`Command` or `Super`
for Cmd, `Control` for Ctrl, as Tauri's accelerator parser accepts)
and teach `accelToDisplayParts` to show them as ⌘ and ⌃. Elsewhere
Ctrl stays `CommandOrControl`; the Windows/Super key needs a decision
(record as `Super` or reject). Extend
`recorder_accelerators_parse_into_distinct_shortcuts` in
`src-tauri/src/gadget_host.rs` with the new tokens.
