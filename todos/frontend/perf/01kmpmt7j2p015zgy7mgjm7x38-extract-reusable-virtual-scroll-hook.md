# Extract reusable virtual scroll hook/component

**Priority: near-term evaluation**

We now have three instances of the same windowed-list pattern:
- `useWindowedList` (launcher `ResultList`)
- `useWindowedGrid` (emoji gadget `EmojiGrid`)
- Clipboard gadget list (upcoming, same technique)

Each reimplements the same core logic: ref-based window position,
keyboard-follow during render, non-passive wheel listener, render-time
slicing, and window reset on result count change.

## Goal

Evaluate whether these can be unified into a single reusable hook
(and possibly a thin wrapper component). The 1D list and 2D grid
variants share ~80% of the logic; the grid just does row-space
arithmetic on top.

## Considerations

- The hook API should stay simple: accept `selectedIndex`,
  `resultCount`, `pageSize` (and optionally `columns` for grid mode),
  return `windowStart` and the slice range.
- Mouse wheel handling could be bundled or left to the consumer.
- Avoid over-abstracting if the variants diverge significantly in
  practice — the current duplication is small and readable.
- Check whether a shared hook would complicate gadget isolation once
  gadgets move to dynamic registration.
