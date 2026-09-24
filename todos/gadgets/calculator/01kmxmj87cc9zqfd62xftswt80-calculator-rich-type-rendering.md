---
kind: feature
status: open
---

# Calculator: rich per-type result rendering

## Problem

The calculator gadget evaluates expressions that can return different types
(int, float, boolean, and potentially string/tuple). Each type could benefit
from distinct visual rendering in the inline result component — e.g., booleans
displayed with color coding (green/red for true/false), floats with precision
controls, integers with thousand separators, etc.

## Current approach

For the initial implementation, the backend passes a `resultType` string tag
alongside the result string in the `data` JSON. The frontend uses this for
basic type-aware styling. All types render as text with minimal differentiation.

## Future improvements

- Booleans: color-coded true/false badges (green/red)
- Integers: optional thousand separators based on locale
- Floats: configurable decimal precision, toggle scientific notation
- Potentially: syntax highlighting for the expression itself
- Consider how type rendering interacts with copy-to-clipboard (always copy
  the plain text result regardless of visual styling)

## Context

Decided during calculator gadget planning. Deferred to keep the initial
implementation focused on core functionality.
