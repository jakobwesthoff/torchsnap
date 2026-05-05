# 8. Use two-tier result display with plugin-claimed views

Date: 2026-03-25

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

The launcher needs to display results from multiple plugins in a
unified way, but some plugins require fundamentally different
presentation than a simple list. A calculator showing a result
inline is not the same as an emoji picker needing a grid layout.
Forcing all plugins into one display model would either limit
plugin capabilities or bloat the standard list with special cases.

Alternatives considered:

* **List-only**: Simple but too limiting for plugins like emoji
  picker or clipboard history that need spatial layouts.
* **Plugin-rendered everything**: Maximum flexibility but destroys
  visual consistency and makes the host responsible for nothing.
* **Tab/category system**: Separate views per plugin category.
  Adds navigation friction and breaks the "type and find" flow.

## Decision

Two-tier display model:

**Default mode** is a ranked list where all plugins contribute
entries with a standard row layout (icon + title + subtitle +
optional metadata). The host owns rendering and ranking.

**Plugin-claimed views** let a plugin take over the display area
via two trigger mechanisms:

1. **Query-triggered (prefix match)**: Plugin claims the view based
   on input pattern (e.g. `=` for calculator). The view either
   replaces or augments the list depending on the view mode.
1. **Enter-triggered (sub-view)**: Pressing Enter on a result opens
   a detail view owned by the plugin instead of executing an action.

Plugins declare one of two view modes:

* **Inline**: Rendered above the result list as a special selectable
  entry. The list remains visible below. The inline view participates
  in normal selection/navigation — arrow keys can select it, Enter
  executes its default action. Good for plugins that augment rather
  than replace the list (calculator result + web search fallbacks).
* **Full**: Replaces the result list entirely. Good for spatial
  layouts (emoji grid, clipboard history).

ESC behavior follows the entry mechanism:

* Prefix-triggered views: ESC dismisses the launcher (no "back").
* Enter-triggered sub-views: ESC goes back to the list. View stack
  depth is always 0 or 1.

Prefix conflicts are resolved first-come-first-serve initially.

## Consequences

* Plugins have a clear contract: return structured data for the
  list, or claim a view with a declared mode. No ambiguity.
* The host controls layout and consistency for the default list.
  Plugins that need custom rendering opt in explicitly.
* The inline/full distinction keeps the architecture simple — just
  two conditional slots in the layout, not an arbitrary nesting
  model.
* Prefix conflict resolution will need revisiting once multiple
  third-party plugins exist. Deferred intentionally to avoid
  over-designing before we have real conflicts.
* Enter-triggered sub-views introduce a one-level navigation stack
  that ESC must handle, adding a small amount of state management
  complexity.