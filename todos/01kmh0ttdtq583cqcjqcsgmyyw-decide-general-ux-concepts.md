# Decide: General launcher UX concepts

## Decisions made

### Query lifecycle

Reset on every show. The user expects a fresh start each time they
summon the launcher. Dismissing without acting also clears the query.

### Navigation model

Keyboard-first, mouse as secondary. Mouse works for clicking results
and scrolling but is never required.

**Mouse hover suppression**: Results appearing under a stationary
cursor must not auto-select. Uses a `mouseActiveRef` gate pattern
(already present in our codebase from nutty):
- Set to `false` on: window focus, query change, keyboard navigation
- Set to `true` on: `onMouseMove` over the list container
- `onMouseEnter` per row only updates selection when the gate is open

### Prefix vs universal search

Both. Universal by default — all plugins see the query and contribute
ranked results. Prefix narrows/claims the view (e.g. `=` for
calculator). Details in
[decide-result-display-model](01kmh0ttdtq583cqcjqcsgmyyt-decide-result-display-model.md).

### Result ordering

Frecency (frequency + recency) as the base signal. User selections
for specific queries are stored, building a learned ranking over
time. Plugin priority is a tiebreaker. Plugins can boost their
results when the query strongly matches their domain.

Details in
[result-ranking-system](01kmh1ah0j1c2cx7pa39rmp7jz-result-ranking-system.md).

### History

Store execution history for frecency ranking. Whether to show
history as a landing state (recent actions on empty query) is
deferred.

### Pinning / favorites

Deferred. See
[pinning-and-favorites](01kmj8z6yx1wakt5qq4pwcbq79-pinning-and-favorites.md).

### Animated transitions

Minimal. Result card height animates smoothly when growing/shrinking.
No per-item entrance animations — they slow perception. Respect
`prefers-reduced-motion`.

### Window and card sizing

The launcher window always covers the full screen (transparent
overlay for click-to-dismiss). The result card has dynamic height
based on result count, with a max height and scroll. The card
grows/shrinks as results change. Empty state can be shorter.

### Multiple monitors

Launcher appears on the monitor where the cursor currently is
(already implemented). No change needed.

### Accessibility / Onboarding

Covered by separate todos:
- [accessibility](01kmh2c7pem81px3twgqhsz4tj-accessibility.md)
- [onboarding-first-run](01kmh2c7pem81px3twgqhsz4th-onboarding-first-run.md)

## UX principles

- **Speed over features**: Every interaction should feel instant.
  If it can't be instant, show progress. Never block the input.
- **Keyboard-first**: The mouse should work but never be required.
  Every action reachable via keyboard.
- **Minimal chrome**: The launcher should feel like it's part of
  the OS, not a separate application. No unnecessary UI elements.
- **Progressive disclosure**: Simple for basic use, powerful
  features discoverable through exploration (prefixes, modifier
  keys, action palette).
- **Consistent behavior**: Same patterns across all plugins.
  Enter always executes. Escape always dismisses. Arrow keys
  always navigate.
