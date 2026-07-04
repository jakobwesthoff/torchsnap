# Destructive system commands execute on a single Enter — confirmation hook exists but is dead code

**Kind:** improvement
**Severity:** medium
**Area:** src-tauri/src/gadgets/system_commands/

## Problem

Restart, Shut Down, Log Out (`macos_commands/power.rs`), and Empty
Trash (`macos_commands/utilities.rs:56-59`) execute immediately
when the user presses Enter on the entry. Empty Trash permanently
deletes data (`tell application "Finder" to empty trash`); the
power commands end the session.

The `SystemCommand` trait already has the hook for this
(`src-tauri/src/gadgets/system_commands/mod.rs:42-47`):

```rust
/// Whether the command requires user confirmation before executing.
/// Deferred — always returns false for now.
#[allow(dead_code)]
fn needs_confirmation(&self) -> bool {
    false
}
```

It is `#[allow(dead_code)]` and never called; no command overrides
it, and the gadget's `execute()` (`mod.rs:108-121`) dispatches
straight to `cmd.execute()`.

A fuzzy launcher makes accidental triggers realistic: "Empty
Trash" and "Eject Disc" share the keyword `disk`/`trash`-adjacent
prefixes with harmless queries, and Enter on the wrong top result
is a one-keystroke mistake.

## Impact

One mistyped Enter can empty the Trash (irreversible) or log the
user out (losing unsaved state in apps that don't handle the
System Events logout gracefully).

## Suggested fix

Decide the confirmation UX (inline confirm state in the launcher,
a second action press, or a modal) and wire `needs_confirmation()`
through the execute path; override it to `true` for Empty Trash,
Restart, Shut Down, and Log Out. If confirmation is deliberately
rejected, remove the dead trait method instead of keeping the
misleading hook.
