# Decide: Result display model

Open architectural decision about how search results are presented
in the launcher.

## Questions to resolve

- **Simple list vs rich display**: Is a flat list of rows sufficient,
  or do some results need richer rendering (preview panes, multi-line
  cards, inline content)?
- **Row anatomy**: What does a standard result row contain?
  - Icon + title only?
  - Icon + title + subtitle/description?
  - Icon + title + subtitle + right-aligned metadata (shortcut hint,
    category badge)?
  - Preview thumbnail?
- **Detail panel**: Should selecting a result show a detail/preview
  panel (like macOS Spotlight's right-side preview)? Or keep it
  minimal like Alfred?
- **Plugin-provided rendering**: Can plugins customize how their
  results look? Options:
  - Plugins return structured data, host renders with standard row
    template (safest, most consistent)
  - Plugins provide HTML fragments (flexible but breaks consistency)
  - Plugins choose from a set of predefined row layouts (middle
    ground)
- **Grouping**: Should results be grouped by source/plugin with
  section headers? Or a single unified ranked list?
- **Empty state**: What shows when there's no query? Recent items?
  Pinned commands? Nothing?
- **Loading state**: How to handle slow plugins? Show results
  incrementally as they arrive? Loading skeleton? Spinner per
  section?

## Prior art

- **Spotlight**: Icon + title + category, detail preview panel on
  right. Grouped by category.
- **Alfred**: Simple icon + title rows, no preview. Unified list.
- **Raycast**: Icon + title + subtitle, optional detail panel.
  Grouped by extension with section headers.
- **LaunchBar**: Single row highlight, info panel below.
