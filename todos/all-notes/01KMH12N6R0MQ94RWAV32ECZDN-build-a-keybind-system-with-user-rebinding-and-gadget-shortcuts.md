---
title: "Build a keybind system with user rebinding and gadget shortcuts"
kind: feature
component: frontend
---
# Build a keybind system with user rebinding and gadget shortcuts

Extend nutty's in-app keybinding infrastructure into a general-purpose
keybind system that supports user configuration and gadget-defined
shortcuts. The in-app engine is already in place (full keybinding
engine, `useKeyBindings` hook, emacs bindings, and launcher ESC and
navigation wiring), so the remaining work is the registry, the user
overrides, the settings page and the gadget API.

## Reference

Nutty uses `KeyBindingProvider` and related hooks from acornkit for
in-app keybindings. The global OS shortcut is separate and handled by
`tauri-plugin-global-shortcut`.

## Requirements

- **Built-in shortcuts**: ESC to dismiss, arrow keys to navigate,
  Enter to execute, Tab for completion, etc.
- **User-configurable**: Users can rebind any shortcut via settings
- **Gadget-extensible**: Gadgets must be able to register their own
  keybindings (e.g. clipboard manager binding Cmd+Shift+V to show
  clipboard history, or a gadget adding Cmd+K for its action palette)
- **Conflict resolution**: What happens when two gadgets claim the
  same keybind? Priority system? User override? Error?
- **Context-aware**: Some keybinds only apply in certain states
  (e.g. within the launcher vs within settings, or when a specific
  gadget result is selected)
- **Discoverability**: Show available keybinds somewhere (footer
  hints, settings page, cheat sheet command)

## Design questions

- Where is the keybind registry? Rust side, JS side, or both?
- Are keybinds stored in the settings store alongside other
  settings, or a separate config file?
- How do gadgets declare their default keybinds in their manifest?
- Should modifier keys on result rows be part of this system or the
  action model? ADR 0055 fixes them per action slot (`primary` Enter,
  `secondary` Cmd+Enter, `copy` Cmd+C, `reveal` Cmd+Shift+R, `delete`
  Cmd+Backspace, `open-settings` Cmd+,), in one frontend table. User
  rebinding would change that table.

## Gadget-defined actions with their own bindings

ADR 0055 defers these. Points raised so far, none decided:

- They would sit next to the fixed slots in `entry-actions`, for
  example as a `custom` list whose items carry a label, a command and
  a requested binding.
- A requested binding can collide with the slot keys, with launcher
  keys (Esc, arrows, Tab, Cmd+K) and with other custom actions. ADR 0009
  makes the host responsible for shortcut assignment, so a gadget can
  only request a binding.
- User rebinding needs a stable name per custom action, for example
  `zerotier.show-members`. Slots are named by their field and do not
  need one. The name is for bindings only. The command still decides
  what runs.
- A settings page that lists rebindable actions must know them without
  running a search, which points to declaring them in the gadget
  manifest and referencing them by name from entries.
- An action without a binding is reachable only through an action
  palette, which does not exist yet.

## Implementation sketch

1. Define a `KeybindRegistry` that maps action IDs to key combos
2. Store user overrides in settings store
3. Provide a `useKeybind(actionId, handler)` hook for components
4. Settings UI page showing all registered keybinds with rebind
   capability
5. Gadget API: `registerKeybind({ id, defaultCombo, description })`
