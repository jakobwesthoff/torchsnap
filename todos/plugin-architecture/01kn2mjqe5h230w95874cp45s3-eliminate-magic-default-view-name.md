# Eliminate Magic "default" View Name

When `execute()` returns `ShowCustomUI`, the frontend synthesizes a
`PluginViewRef` with `view: "default"` (`Launcher.tsx:213-216`). This magic
string is also the key in the plugin registry for execute-triggered plugins
(e.g., clipboard).

Similarly, the `activate-plugin-custom-ui` event from shortcuts carries only
`plugin_id`, no view name — the frontend again synthesizes `"default"`.

This means:
- Shortcuts can only activate the `"default"` view
- Execute-triggered custom UI can only show the `"default"` view
- The string `"default"` is a contract shared between `Launcher.tsx` and the
  registry with no type-level declaration

Options:
- Define a constant (`DEFAULT_PLUGIN_VIEW = "default"`) shared across both
  files
- Have the backend send an explicit view name in `PostAction::ShowCustomUI`
  and in the shortcut activation event, eliminating the frontend synthesis
- Both of the above
