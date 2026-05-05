# 10. Use keyboard-first interaction model with frecency ranking

Date: 2026-03-25

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

The launcher's interaction model shapes every user-facing feature.
Decisions about query lifecycle, input handling, result ranking, and
window behavior are deeply interconnected and must be settled
together to avoid contradictory UX patterns.

Key tensions:

* **Speed vs discoverability**: Power users want zero friction;
  new users need to learn how the launcher works.
* **Mouse vs keyboard**: Supporting both without either feeling
  like an afterthought.
* **Static ranking vs learned ranking**: Fixed plugin priority is
  predictable but doesn't adapt; ML-style ranking adapts but can
  feel unpredictable.

## Decision

### Query lifecycle

The query resets on every show. Users expect a fresh start each
time they summon the launcher. Dismissing without acting also
clears. No query persistence or history recall on show.

### Navigation model

Keyboard-first. Mouse works for clicking results and scrolling but
is never required. Every action is reachable via keyboard alone.

**Mouse hover suppression** prevents results that appear under a
stationary cursor from auto-selecting. A `mouseActiveRef` gate:

* Set to `false` on window focus, query change, keyboard navigation
* Set to `true` only on `onMouseMove` over the list container
* `onMouseEnter` per row only updates selection when gate is open

### Result ranking

Frecency (frequency + recency) as the primary signal. The system
stores which results the user selects for which queries, building
learned preferences over time. Plugin-declared priority is a
tiebreaker. Plugins can boost results when the query strongly
matches their domain (e.g. a URL plugin boosts when input looks
like a URL).

### Window and card sizing

The launcher window always covers the full screen as a transparent
overlay (click outside dismisses). The result card within has
dynamic height based on result count, with a max height and
scrolling. The card grows/shrinks as results change. Appears on the
monitor where the cursor is.

### Animations

Minimal. Card height transitions smoothly. No per-item entrance
animations. Respect `prefers-reduced-motion`.

### UX principles

1. Speed over features — every interaction feels instant
1. Keyboard-first — mouse works but is never required
1. Minimal chrome — feels like part of the OS
1. Progressive disclosure — simple by default, power through
   exploration
1. Consistent behavior — Enter executes, Escape dismisses, arrows
   navigate, across all plugins

## Consequences

* Frecency ranking requires persistent storage of user selections
  and a scoring algorithm. Adds complexity compared to static
  ranking, but produces a launcher that improves with use.
* Query reset on show means no "resume where I left off" capability.
  This is intentional — the launcher is for quick lookups, not
  sustained workflows.
* Mouse hover suppression adds a ref-based gate pattern to every
  component that renders result rows. Small implementation cost for
  a significant UX improvement.
* Dynamic card height requires measuring content and animating
  smoothly. More complex than fixed height, but avoids wasted space
  and oversized empty states.
* The keyboard-first principle means every new feature must be
  audited for keyboard reachability. This is a design constraint,
  not just a preference.