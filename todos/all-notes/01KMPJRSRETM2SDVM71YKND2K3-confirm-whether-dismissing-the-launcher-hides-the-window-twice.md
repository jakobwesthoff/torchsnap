---
title: "Confirm whether dismissing the launcher hides the window twice"
kind: investigation
component: frontend
tags: [ux]
---
# Confirm whether dismissing the launcher hides the window twice

The launcher hide command may fire twice when the user dismisses the launcher with Escape or by clicking the transparent backdrop. This is not confirmed yet. If it happens, the duplicate needs to be traced to its origin before anyone fixes it.

## Context

Two likely sources:

- **Escape key.** The keybinding engine and the blur handler of `useWindowLifecycle` (`src/launcher/hooks/useWindowLifecycle.ts`) may both trigger `dismiss()`. Pressing Escape might call `dismiss()` directly through the keybinding, which hides the window, which fires `tauri://blur`, which calls `dismiss()` again.
- **Backdrop click.** The `onClick={dismiss}` on the backdrop div (`src/launcher/Launcher.tsx`) hides the window, which fires `tauri://blur`, which calls `dismiss()` a second time.

## What to check

1. Add a `console.log` or a counter in `dismiss()` to confirm it fires twice per interaction.
2. Trace both code paths (keybinding, then `dismiss`, then hide, then blur, then `dismiss`) to identify the duplicate.
3. Pick a guard against the double fire: check the visibility state before hiding, or debounce the dismiss callback.
