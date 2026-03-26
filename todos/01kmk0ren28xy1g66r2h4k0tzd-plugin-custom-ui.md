# Plugin custom UI

Allow plugins to provide their own frontend rendering instead of
using the standard result list. This enables richer displays like
emoji grids, calculator inline results, or preview panes.

## Motivation

The standard vertical result list works for most plugins, but some
benefit from fundamentally different layouts:
- **Emoji picker**: grid of emoji (show ~50+ at once vs ~10 in a
  list)
- **Calculator**: inline result display (expression → answer, no
  list at all)
- **Color picker**: swatch grid with preview
- **File preview**: result list + side preview pane

Without this, every plugin is constrained to "list of rows with
icon + title + subtitle."

## Design questions

- **How does a plugin declare its UI?** A React component bundled
  with the plugin? A display mode hint ("grid", "inline")? A
  template system?
- **Isolation**: should plugin UI run in an iframe/shadow DOM for
  style isolation, or share the main renderer?
- **Base library**: plugins should have access to shared components
  (KeyBindingPill, icons, theme tokens) so they look native
- **Data flow**: plugin UI still receives the same search results
  and keybinding infrastructure, just renders differently
- **Hybrid mode**: can a plugin use custom UI for some states (empty
  query → grid) and standard list for others (filtered → list)?

## Approach options

- **A) Display mode hints**: plugin metadata specifies `displayMode:
  "list" | "grid" | "inline"`. Built-in renderers handle each mode.
  Simple, limited, but covers 80% of cases.
- **B) Plugin-provided React components**: plugins bundle a React
  component that receives results + actions as props. Maximum
  flexibility, but requires a component loading/sandboxing system.
- **C) Template DSL**: plugins describe layout via a declarative
  schema. Safe but limited expressiveness.

Option A is probably the right first step — build grid and inline
as built-in display modes, then evolve toward B if needed.

## Depends on

- decide-plugin-architecture — the overall plugin system design
- plugin-emoji-picker — the primary motivating use case
