---
kind: bug
severity: medium
status: open
area: [src-tauri/src/gadget_host.rs]
tags: [unconfirmed]
---

# register_all_shortcuts: one bad combo (or missing store key) disables all shortcuts

## Problem
`register_all_shortcuts` (`src-tauri/src/gadget_host.rs:533-614`)
has an all-or-nothing structure with several brittle edges:

1. **Panic on missing/mistyped `globalShortcut`.** Line 554-559:

   ```rust
   let launcher_combo_str = self
       .store
       .get("globalShortcut")
       .and_then(|v| v.as_str().map(String::from))
       .expect("globalShortcut initialized by settings defaults");
   ```

   The value comes from the mutable settings store; the frontend
   or a hand-edited store file can delete the key or store a
   non-string. The function runs inside the shortcut reactor
   task (`start_shortcut_reactor`,
   `gadget_host.rs:494-515`), so the panic kills that task
   permanently — no further shortcut re-registration for the
   rest of the session, with no user-visible signal.

2. **Invalid launcher combo strands everything unregistered.**
   `unregister_all()` runs first (line 536); if the stored
   launcher combo fails to parse, the function `return`s at line
   561-567 — after the unregister, before any registration. All
   gadget shortcuts and the launcher toggle are gone until the
   next successful re-registration.

3. **Bulk `on_shortcuts` is all-or-nothing.** Every combo
   (launcher + all gadget shortcuts) is registered in a single
   call (lines 578-580). If the plugin rejects the batch — e.g.
   two gadgets resolve to the same combo (nothing deduplicates
   `all_combos`), or a combo is already taken at the OS level —
   the error path is a single `eprintln` and the app runs with
   zero working shortcuts, including the launcher toggle, which
   is the primary way to open the app.

## Impact
A single malformed or conflicting shortcut string (user-editable
data) can take down the launcher's global activation shortcut —
the app becomes unreachable-by-keyboard until the user changes a
setting that happens to retrigger a successful registration.

## Suggested fix
Parse/validate combos *before* `unregister_all`. Register
shortcuts individually (or fall back to individual registration
when the bulk call fails) so one bad combo only loses itself.
Deduplicate `all_combos` with a deterministic winner and log the
conflict. On launcher-combo parse failure, fall back to the
compiled-in default rather than returning early. Replace the
`expect` with the same fallback.
