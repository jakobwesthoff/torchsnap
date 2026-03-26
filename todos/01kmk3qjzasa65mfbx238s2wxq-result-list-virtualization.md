# Result list virtualization or limit

The emoji picker can return hundreds of results for broad queries
(e.g., `:s` matches most shortcodes). Rendering all of them as DOM
nodes causes visible lag/jank in the launcher.

## Options

- **Virtual scrolling**: only render the ~15-20 visible rows plus a
  small buffer. Libraries like `@tanstack/react-virtual` or a simple
  custom implementation. This is the proper long-term fix.
- **Hard result limit**: cap results at e.g. 50-100 entries. Simple
  but loses tail results. Could be a stopgap until virtualization
  is implemented.
- **Combination**: virtualize the list AND cap at a reasonable limit
  (e.g., 200) since users won't scroll through thousands of emoji.

## Context

This affects all plugins, not just emoji — the app launcher with
~180 apps was fine, but emoji can produce 1000+ matches. Any future
plugin with a large result set (file search, clipboard history) will
hit the same issue.

## Recommendation

Implement virtual scrolling. It's a one-time investment that removes
the DOM node count as a performance constraint entirely. A hard limit
can be added as well but shouldn't be the only solution.
