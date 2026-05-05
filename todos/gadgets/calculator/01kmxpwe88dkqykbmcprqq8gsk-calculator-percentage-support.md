# Calculator: percentage syntax support

## Problem

Users expect `100*5%` to evaluate as `100 * 0.05 = 5`, and `200 + 15%` to
possibly mean `200 + (200 * 0.15) = 230`. The `%` operator is not valid
`evalexpr` syntax, so percentage support requires preprocessing.

## Approach

Add a preprocessing step that expands `%` in expressions before passing to
`evalexpr`. The simplest form: `N%` → `(N/100)` via regex substitution. More
advanced: `X + N%` → `X + X * (N/100)` for "add N percent" semantics.

## Considerations

- Simple replacement (`N%` → `N/100`) is straightforward but doesn't cover
  the "percentage of" pattern (`200 + 15%` meaning "200 plus 15% of 200").
- Need to decide which percentage semantics to support.
- The heuristic detection rules reference percentages — update those when
  implementing (require left operand: `100*5%` yes, bare `5%` no).
- Percentage expressions should also be covered by the heuristic detection
  regex.

## Context

Deferred during calculator plugin planning to keep initial implementation
focused. Percentages were originally planned as part of the preprocessing
layer but separated out as a distinct feature.
