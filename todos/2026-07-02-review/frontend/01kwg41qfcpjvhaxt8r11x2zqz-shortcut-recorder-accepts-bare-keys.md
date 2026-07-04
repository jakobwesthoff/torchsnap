# ShortcutRecorder commits modifier-less combos — a bare keypress becomes the global hotkey

**Kind:** bug
**Severity:** high

**Area:** src/components/ShortcutRecorder.tsx

## Problem
The component's header comment promises that recording "commits on
keyup when a complete combo (modifier + non-modifier) is detected"
(`src/components/ShortcutRecorder.tsx:10-11`). The completeness check
does not require a modifier (`ShortcutRecorder.tsx:66-68`):

```ts
function isComplete(e: KeyboardEvent): boolean {
  return !MODIFIER_CODES.has(e.code);
}
```

and `buildAccelerator` (`ShortcutRecorder.tsx:46-64`) happily
produces a single-token accelerator when no modifier is held —
pressing just `A` yields `"A"`, `Enter` yields `"Enter"`. The keydown
handler then stores it as pending (`ShortcutRecorder.tsx:167-169`)
and the next keyup commits it via `onChange`
(`ShortcutRecorder.tsx:172-180`).

In the settings panel this flows straight into the `globalShortcut`
setting (`src/settings/ShortcutSection.tsx:27-37` →
`setGlobalShortcut(combo)`), which the backend shortcut reactor
registers as a *system-wide* accelerator
(`src-tauri/src/gadget_host.rs`, `register_all_shortcuts`). A single
key is a valid accelerator for the global-shortcut plugin, so the OS
grabs that key globally.

The commit gesture is also the easiest accident available: while in
recording mode, any lone keypress (a stray letter, Enter, Space)
immediately commits. Only Escape is special-cased as cancel
(`ShortcutRecorder.tsx:157-160`).

Compounding this, registration errors never surface here:
`ShortcutSection`'s `catch` only sees settings-store write failures;
the actual registration happens asynchronously in the backend
reactor, whose failure modes are logged with `eprintln` at best (see
`host-core/01kwg1ph0qcdqtara5jcw7abyj-shortcut-reregistration-failure-modes.md`).
So a bad combo is accepted with no feedback path at all.

## Impact
A user who enters recording mode and presses a single key hijacks
that key system-wide: every press of e.g. `a` in any application
toggles the launcher instead of typing. Recovering requires
re-recording the shortcut (while the key itself no longer types).
Combined with the backend's all-or-nothing registration, an
unregisterable value can strand all shortcuts.

## Suggested fix
Enforce the documented contract: require at least one modifier
before setting `pendingRef` (e.g.
`(e.metaKey || e.ctrlKey || e.altKey) && !MODIFIER_CODES.has(e.code)`
— Shift-only is also questionable for a global hotkey). Keep the
live preview for incomplete chords but do not commit them.
