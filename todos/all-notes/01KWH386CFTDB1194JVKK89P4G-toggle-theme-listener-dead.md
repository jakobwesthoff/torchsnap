---
kind: bug
severity: low
status: open
area: [src/contexts/ThemeProvider.tsx]
tags: [unconfirmed]
---

# ThemeProvider listens for a `toggle-theme` event that nothing emits

## Problem

`ThemeProvider` registers a Tauri event listener
(`src/contexts/ThemeProvider.tsx:94-112`):

```ts
// Listen for the toggle-theme event emitted by the built-in
// commands gadget. Switches between dark and light only — ...
useEffect(() => {
  const unlisten = listen("toggle-theme", () => { ... });
```

No emitter exists. Verified 2026-07-02 by searching the whole
repo for `toggle-theme` / `toggle_theme`: the only hit is this
listener. The comment attributes the emit to "the built-in
commands gadget", but `src-tauri/src/gadgets/commands.rs`
contains no theme-related code, and the system-commands
appearance module
(`src-tauri/src/gadgets/system_commands/macos_commands/appearance.rs`)
toggles the *OS* appearance, not this app-internal event.

## Impact

No runtime breakage — the listener simply never fires. But:

- A "toggle theme" launcher command that the comment presents
  as existing is actually absent; either the feature was lost
  in a refactor (the commands gadget predates the WASM
  migration) or never ported. If the feature is wanted, it is
  silently missing.
- The dead listener plus its misleading comment send the next
  reader hunting for an emitter that does not exist.

## Suggested fix

Decide the feature's fate:

- If a theme-toggle launcher command should exist, add it to
  the commands gadget (emit `toggle-theme` on execute) — the
  frontend half is already wired and behaves sensibly
  (dark ↔ light flip, "system" resolved to its effective
  value first).
- If not, delete the listener effect and its comment.
