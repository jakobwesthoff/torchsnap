# Transfer Emacs keybindings hook

Nutty supports Emacs-style keybindings in the search input (Ctrl+A,
Ctrl+E, Ctrl+K, Ctrl+U, etc.) via the `useEmacsBindings` hook from
acornkit.

## Reference

`useEmacsBindings(inputRef, setQuery)` in acornkit returns an object
spread onto the input as event handlers. It intercepts Ctrl-prefixed
keys and manipulates the input selection/value directly.

## What needs to happen

1. Transfer or rewrite the `useEmacsBindings` hook
2. Wire it into the launcher's search input
3. This should be part of the shared component library extraction
   (see separate todo)
