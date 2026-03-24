# Decide: Action model for result entries

Open architectural decision about whether result entries support
a single action or multiple actions, and how users invoke them.

## Questions to resolve

- **Single vs multiple actions**: Does Enter always do the same
  thing, or can an entry offer multiple actions (open, copy path,
  reveal in Finder, delete, etc.)?
- **Action invocation**: If multiple actions, how does the user
  pick one?
  - Tab or arrow-right to expand an action bar/submenu?
  - Modifier keys (Cmd+Enter, Shift+Enter, Alt+Enter)?
  - Right-click context menu?
  - Action palette (like Raycast's Cmd+K)?
- **Default action**: Which action runs on plain Enter? Is it
  always the first? Can the user configure it per result type?
- **Plugin-defined actions**: Can plugins declare custom actions
  beyond the standard set? How are custom action shortcuts
  assigned without conflicts?
- **Action discoverability**: How does the user learn what actions
  are available? Visible hints in the row? Only on hover/focus?
  A persistent footer showing available shortcuts?

## Prior art

- **Alfred**: Single default action on Enter. Modifier keys for
  alternatives (Cmd+Enter, Alt+Enter). Actions configurable in
  workflow editor.
- **Raycast**: Primary action on Enter, Cmd+K opens full action
  panel with a searchable list. Multiple actions per entry.
- **Spotlight**: Single action only — Enter opens/launches.
- **LaunchBar**: Tab to "send to" another action. Composable
  action chains.

## Tradeoffs

- Single action is simpler to implement and harder to confuse
  users, but limits power users.
- Multiple actions add complexity but make the launcher a true
  productivity tool (copy vs open vs reveal etc.).
- Modifier keys are discoverable only if hinted, but feel native
  to keyboard-centric users.
- A Raycast-style action palette is the most discoverable but adds
  UI complexity and an extra keypress.
