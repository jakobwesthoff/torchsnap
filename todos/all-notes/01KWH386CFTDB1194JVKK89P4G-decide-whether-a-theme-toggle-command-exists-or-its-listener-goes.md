---
title: "Decide whether a theme toggle command exists or its listener goes"
kind: investigation
component: frontend
status: needs-discussion
impact: low
tags: [ux]
---
# Decide whether a theme toggle command exists or its listener goes

`ThemeProvider` registers a Tauri event listener for `toggle-theme` (`src/contexts/ThemeProvider.tsx:94-112`), and nothing emits that event. The listener never fires, so there is no runtime breakage. The listener's comment attributes the emit to "the built-in commands gadget", which presents a theme toggle as existing when it does not. Whether that feature should exist is a decision only the user can make.

```ts
// Listen for the toggle-theme event emitted by the built-in
// commands gadget. Switches between dark and light only — ...
useEffect(() => {
  const unlisten = listen("toggle-theme", () => { ... });
```

## Context

Verified 2026-07-02 by searching the whole repository for `toggle-theme` and `toggle_theme`: the only hit is this listener. The comment attributes the emit to the built-in commands gadget, but `src-tauri/src/gadgets/commands.rs` contains no theme-related code. The system-commands appearance module (`src-tauri/src/gadgets/system_commands/macos_commands/appearance.rs`) toggles the OS appearance, not this app-internal event.

## Impact

- A "toggle theme" launcher command that the comment presents as existing is absent. Either the feature was lost in a refactor (the commands gadget predates the WASM migration) or it was never ported. If the feature is wanted, it is silently missing.
- The dead listener and its misleading comment send the next reader hunting for an emitter that does not exist.

## Options

- **Add the feature.** Add a theme toggle to the commands gadget that emits `toggle-theme` on execute. The frontend half is already wired and behaves sensibly: dark and light flip, with "system" resolved to its effective value first.
- **Drop it.** Delete the listener effect and its comment.
