# Keybind system

Transfer nutty's keybinding infrastructure and extend it into a
general-purpose keybind system that supports user configuration
and plugin-defined shortcuts.

## Reference

Nutty uses `KeyBindingProvider` and related hooks from acornkit for
in-app keybindings (separate from the global OS shortcut which is
handled by `tauri-plugin-global-shortcut`).

## Requirements

- **Built-in shortcuts**: ESC to dismiss, arrow keys to navigate,
  Enter to execute, Tab for completion, etc.
- **User-configurable**: Users can rebind any shortcut via settings
- **Plugin-extensible**: Plugins must be able to register their own
  keybindings (e.g. clipboard manager binding Cmd+Shift+V to show
  clipboard history, or a plugin adding Cmd+K for its action palette)
- **Conflict resolution**: What happens when two plugins claim the
  same keybind? Priority system? User override? Error?
- **Context-aware**: Some keybinds only apply in certain states
  (e.g. within the launcher vs within settings, or when a specific
  plugin result is selected)
- **Discoverability**: Show available keybinds somewhere (footer
  hints, settings page, cheat sheet command)

## Design questions

- Where is the keybind registry? Rust side, JS side, or both?
- Are keybinds stored in the settings store alongside other
  settings, or a separate config file?
- How do plugins declare their default keybinds in their manifest?
- Should modifier keys on result rows (Cmd+Enter for secondary
  action) be part of this system or the action model?

## Implementation sketch

1. ~~Transfer `KeyBindingProvider` from acornkit~~ ✓ (done — full
   keybinding engine, `useKeyBindings` hook, emacs bindings, and
   launcher ESC/navigation wiring are in place)
2. Define a `KeybindRegistry` that maps action IDs to key combos
3. Store user overrides in settings store
4. Provide a `useKeybind(actionId, handler)` hook for components
5. Settings UI page showing all registered keybinds with rebind
   capability
6. Plugin API: `registerKeybind({ id, defaultCombo, description })`
