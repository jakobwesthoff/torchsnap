# Key press handling generalization

The launcher needs a generalized keyboard event handling system
rather than ad-hoc `onKeyDown` handlers scattered across components.

## Reference

Nutty uses `useKeyboardNavigation` from acornkit which handles:
- Arrow up/down for list navigation
- Enter to execute selected item
- Tab for completion
- Scroll-into-view for the selected item
- Mouse vs keyboard mode tracking

## What needs to happen

1. Review nutty's `useKeyboardNavigation` hook in acornkit
2. Adapt or rewrite for torchsnap's needs
3. Centralize all keyboard shortcuts in the launcher into a single
   system that can be extended by plugins later
4. Consider a keybinding registry pattern so plugins can register
   their own shortcuts
