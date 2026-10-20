---
title: "Calculator heuristic evaluates slash dates as division"
kind: bug
component: gadgets
impact: low
origin: implementation
tags: [ux]
---
# Calculator heuristic evaluates slash dates as division

Typed without the `=` prefix, a date written with slashes such as `01/15/2024` or `2024/01/15` shows an arithmetic result above the real results (0.0000329 and 134.93). Found while fixing the hyphenated-date case on 2026-10-20 (commit 7739ff6d), left out of that fix.

## Problem

`has_identifier_shaped_token` in `gadgets/calculator/src/lib.rs` only looks at digit groups joined by hyphens. Slash dates reach the division heuristic unchanged. Their shape is the same as chained division such as `100/5/2`, so a rule cannot just veto every three-group slash token.

## Impact

Cosmetic but recurring, like the hyphenated case: users typing a date get a meaningless number pinned above their results. Heuristic mode is on by default.

## Suggested fix

Veto slash tokens that read as a date: three groups where one has four digits and the others one or two digits, or two groups of one or two digits followed by a four-digit group. Keep `100/5/2` and `10/4` evaluating; pin both with tests, and add `01/15/2024` and `2024/01/15` as non-match tests. The `=` prefix keeps evaluating them.
