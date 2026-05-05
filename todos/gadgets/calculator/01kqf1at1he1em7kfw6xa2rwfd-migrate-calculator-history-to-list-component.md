# Migrate Calculator History to Shared `<List>` SDK Component

**Status: DEFERRED — exploratory follow-up, not blocking.**

## Context

During the ZeroTier gadget work (todo
`01kqaqzdma111a817snbna5n8b-zerotier-one-integration.md`) a new
shared `<List>` component was added to the gadget SDK
(`packages/gadget-sdk/src/shims/components.ts`, host implementation
at `src/components/List.tsx`). The ZeroTier settings page is its
first consumer, used to render the "remembered networks" management
view.

The calculator gadget already renders a list of records — its
history view at
`gadgets/calculator/frontend/src/views/CalculatorView.tsx:248-300`
— but it predates the shared component and rolls its own DOM.
It was actually the reference DOM shape used while designing
`<List>`.

This todo records the question of whether the calculator history
view should migrate to the shared component, so other gadgets'
list views stay consistent with each other.

## Why this is exploratory and not mandatory

The two views may diverge enough by the time both are mature
that forcing them onto the same primitive would be a regression:

- ZeroTier's list is a settings-page management surface (rare
  interaction, per-row Forget action, no virtualization needed at
  realistic scales).
- Calculator's list is a launcher result surface (high-frequency
  interaction, virtualized scroll via `useWindowedList`, integrated
  with the launcher's keyboard navigation).

A shared component that handles both well is a valuable design
target; a shared component that papers over real differences is a
maintenance liability. Decide after both consumers have shipped
and the component's API has settled in real use.

## When to revisit

- After the ZeroTier gadget has shipped and the `<List>` API has
  been used in anger for at least one minor version cycle.
- If a third gadget needs a list view — that's the strong signal
  that the shared primitive is justified.
- If the two divergent implementations cause a UX inconsistency
  (e.g. different keyboard handling, different selection styling)
  that users notice.

## When to drop this todo

If the `<List>` component evolves in a direction that makes the
calculator's needs awkward (e.g. opinionated row chrome that
calculator doesn't want), close this todo as "diverged
intentionally" and move on. The shared component does not need
to absorb every list-shaped UI in the codebase.

## Implementation outline (when we do it)

- Replace the manual `div + .map()` row rendering at
  `CalculatorView.tsx:248-300` with `<List>`.
- Verify `useWindowedList` integration via the component's
  `virtualized` prop still produces equivalent scroll behaviour.
- Verify keyboard navigation (selected-row highlight, action
  invocation) matches the existing UX.
- Verify the highlight-positions on title / subtitle still render
  correctly through the new component.
- Snapshot-test the rendered output before / after.
