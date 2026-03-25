# Decide: Action model for result entries

## Decisions made

### Multiple actions per entry with clear hierarchy

Each result entry supports multiple actions provided by its plugin.
Three invocation tiers:

1. **Enter** — primary action. Plugin decides what this is (launch
   app, open URL, copy result). Always the first action in the
   plugin's declared list.
2. **Modifier keys** — standardized secondary actions for common
   operations:
   - `Cmd+Enter` / `Ctrl+Enter` — secondary action (e.g. "reveal
     in Finder" for apps, "copy URL" for web results)
   - `Cmd+C` — copy (universally understood, should work on any
     entry that has copyable content)
   - Additional modifiers (`Alt+Enter`, `Shift+Enter`) available
     for the 3rd/4th actions
3. **Action palette** (`Cmd+K` and `Tab`) — opens a searchable list
   of all available actions for the selected entry. Covers
   plugin-specific custom actions that don't have a modifier key.

### Discoverability via footer bar

A footer bar at the bottom of the result list shows contextual
action hints for the currently selected entry. Displays the primary
action label and 1-2 modifier key shortcuts. Updates as selection
changes. The action palette provides full discoverability for all
actions.

### Plugin action API

Plugins declare an ordered list of actions per entry:

- First action = default (Enter)
- Each action has: id, label, optional icon, optional default
  keybinding
- The host assigns modifier keys for the top 2-3 actions
  automatically
- Remaining actions are accessible through the action palette

### Standardized action IDs

Common action types should use standardized IDs so the host can
apply consistent keybindings across plugins:

- `open` — primary open/launch/execute
- `copy` — copy value to clipboard
- `reveal` — show in file manager
- `open-with` — open with alternative app
- `delete` — remove/trash

Plugins can define additional custom action IDs beyond these.

## Still open

- Exact modifier key assignments — finalize once we have real
  plugins exercising the system
- Whether users can rebind action shortcuts per result type
- How plugin-defined action keybindings interact with the
  keybinding engine's conflict resolution

## Prior art

- **Alfred**: Single default action on Enter. Modifier keys for
  alternatives (Cmd+Enter, Alt+Enter). Actions configurable in
  workflow editor.
- **Raycast**: Primary action on Enter, Cmd+K opens full action
  panel with a searchable list. Multiple actions per entry.
- **Spotlight**: Single action only — Enter opens/launches.
- **LaunchBar**: Tab to "send to" another action. Composable
  action chains.
