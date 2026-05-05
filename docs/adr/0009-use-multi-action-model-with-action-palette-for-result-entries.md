# 9. Use multi-action model with action palette for result entries

Date: 2026-03-25

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

Result entries need to support more than just "open/launch." A file
result should offer open, copy path, reveal in Finder, trash. A URL
result should offer open, copy URL, open in incognito. A single
Enter-to-execute model would force users to leave the launcher for
secondary operations.

Alternatives considered:

- **Single action (Spotlight model)**: Simplest, but limits the
  launcher to a glorified app switcher. Power users would find it
  frustrating.
- **Modifier keys only (Alfred model)**: Fast for muscle memory,
  but undiscoverable. Users must memorize or look up shortcuts.
- **Action palette only (no modifier keys)**: Discoverable but
  adds an extra keypress for common operations.

## Decision

Three invocation tiers for actions:

1. **Enter** — primary action. Always the first action in the
   plugin's declared list. Plugin decides what it does.
2. **Modifier keys** — standardized secondary actions:
   - `Cmd+Enter` / `Ctrl+Enter` for secondary action
   - `Cmd+C` for copy (universal)
   - `Alt+Enter`, `Shift+Enter` for 3rd/4th actions
   The host assigns modifier keys to the top actions automatically.
3. **Action palette** (`Cmd+K` and `Tab`) — searchable list of all
   available actions for the selected entry. Covers plugin-specific
   custom actions without dedicated shortcuts.

A **footer bar** at the bottom of the result list provides
discoverability: shows the primary action label and 1-2 modifier
hints for the selected entry, updating as selection changes.

Plugins declare an ordered list of actions per entry, each with:
id, label, optional icon, optional default keybinding. Standardized
action IDs (`open`, `copy`, `reveal`, `open-with`, `delete`) ensure
consistent keybindings across plugins.

## Consequences

- Users get fast single-keypress execution for common operations
  and full discoverability via the palette. Progressive disclosure.
- The standardized action IDs create a shared vocabulary. Plugins
  that use `copy` get `Cmd+C` automatically without declaring it.
- The footer bar is a lightweight discoverability mechanism that
  doesn't clutter the result rows.
- Modifier key assignments are host-controlled, preventing plugins
  from conflicting on shortcuts. Custom plugin shortcuts go through
  the action palette or the keybinding engine.
- The action palette adds UI complexity (a second overlay/list) and
  needs its own keyboard navigation, search, and dismiss behavior.
- Two triggers for the palette (`Cmd+K` and `Tab`) accommodates
  both Raycast-familiar users and keyboard-first flow preferences.
