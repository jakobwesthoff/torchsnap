# Decide: Result display model

## Decisions made

### Two-tier display model

**Default mode: ranked list** — All plugins contribute entries to a
single mixed list, ranked by the host. Standard row layout: icon +
title + subtitle + optional right-side metadata. Covers the vast
majority of use cases (app launcher, web search, contacts, file
search, system commands).

**Plugin-claimed views** — Plugins can take over the display area
with a custom view. Two trigger mechanisms:

1. **Query-triggered (prefix match)**: Plugin claims the view based
   on input pattern (e.g. `=` for calculator). Replaces the list
   entirely with whatever the plugin renders.
2. **Enter-triggered (sub-view)**: Pressing Enter on a result opens
   a detail/sub-view owned by that plugin instead of executing an
   action. Examples: contact card, clipboard history, emoji grid.

### ESC behavior follows entry mechanism

- **Prefix-triggered views**: ESC dismisses the launcher (no "back"
  — the user typed into the search bar, there's nothing to go back
  to).
- **Enter-triggered sub-views**: ESC goes back to the list. A second
  ESC dismisses the launcher. View stack depth is always 0 or 1.

### Plugin view modes: inline vs full

Plugins that claim a custom view declare one of two modes:

- **Inline**: Rendered in a slot above the result list. The list
  remains visible below. Good for plugins that augment rather than
  replace the list (e.g. calculator showing a result while web
  search fallbacks remain).
- **Full**: Replaces the result list entirely. The plugin gets the
  full display area. Good for plugins that need spatial layout
  (emoji grid, clipboard history browser, contact detail).

Inline views are selectable — they participate in the normal
selection/navigation system as a special entry at the top of the
list. Arrow keys can select them, Enter executes their default
action (e.g. copy calculator result), and they can declare
secondary actions like any other entry. The plugin just gets more
control over how that entry renders. This avoids a separate
interaction model for inline views.

For prefix-triggered plugins, the mode is part of the plugin's
prefix registration. For enter-triggered sub-views, full is the
default (the user navigated into something specific).

### Prefix conflict resolution

First-come-first-serve for now. See
[query-prefix-conflict-resolution](01kmj7jdvts1mqa4wcd4mr4qq8-query-prefix-conflict-resolution.md)
for future improvements.

## Still open

- **Row anatomy details**: Exact fields, sizing, truncation behavior.
  Will emerge from building the first plugins.
- **Plugin-provided rendering**: Structured data with host templates
  vs plugin-provided components. Defer until plugin system design.
- **Grouping**: Single ranked list vs grouped by plugin with section
  headers. Leaning towards unified list.
- **Empty state**: What shows with no query (recent items, pinned
  commands, nothing).
- **Loading state**: Incremental results vs skeleton vs spinner.

## Prior art

- **Spotlight**: Icon + title + category, detail preview panel on
  right. Grouped by category.
- **Alfred**: Simple icon + title rows, no preview. Unified list.
- **Raycast**: Icon + title + subtitle, optional detail panel.
  Grouped by extension with section headers.
- **LaunchBar**: Single row highlight, info panel below.
