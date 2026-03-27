# Investigate double hide command on launcher dismiss

The launcher hide command appears to fire twice when dismissing via
Escape or clicking the transparent backdrop area. Needs investigation
to confirm whether it's actually happening and, if so, where the
duplicate originates.

## Likely sources

- **Escape key**: The keybinding engine and `useWindowLifecycle` blur
  handler may both trigger `dismiss()`. Pressing Escape might call
  `dismiss()` directly via the keybinding, which hides the window,
  which fires `tauri://blur`, which calls `dismiss()` again.
- **Backdrop click**: The `onClick={dismiss}` on the backdrop div
  hides the window, which fires `tauri://blur`, which calls `dismiss()`
  a second time.

## What to check

1. Add a `console.log` or counter in `dismiss()` to confirm it fires
   twice per interaction.
2. Trace both code paths (keybinding → dismiss → hide → blur → dismiss)
   to identify the duplicate.
3. Guard against double-fire (e.g., check visibility state before
   hiding, or debounce the dismiss callback).
