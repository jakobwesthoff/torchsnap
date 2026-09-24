---
kind: bug
severity: low
status: open
area: [gadgets/calculator/src/lib.rs]
tags: [unconfirmed]
---

# Calculator heuristic treats dates and phone-style numbers as subtraction

## Problem

The heuristic-mode detector fires on any `digit - digit` pattern
(`gadgets/calculator/src/lib.rs:504-507`):

```rust
static SUBTRACTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d\s*-\s*\d").expect("subtraction regex"));
```

Common non-math launcher inputs match it and successfully
evaluate, so an inline calculator result appears above the search
results:

- ISO dates: `2024-01-15` evaluates as `2024 - 1 - 15` → inline
  result **2008**.
- Phone-style numbers: `555-1234` → inline result **-679**.
- Ticket/issue ids like `JIRA-1234-5678` partially match too
  (the `\d-\d` core), though evaluation then fails and the entry
  is suppressed — only inputs that *evaluate* leak through
  (`heuristic_mode_search`, `lib.rs:212-237`).

The test suite covers `v1.2.3` and `VS Code 2` as non-matches
(`lib.rs:766-774`) but has no date or phone-number cases, so the
gap is untested rather than decided.

## Impact

Cosmetic but recurring: users typing dates or numeric ids into
the launcher get a meaningless arithmetic result pinned above
their actual results. Heuristic mode is on by default
(`heuristicEnabled` default `true`, `lib.rs:94`).

## Suggested fix

Narrow the subtraction heuristic rather than the evaluator.
Options (combinable):

- Require whitespace around the minus (`\d\s+-\s+\d`) so compact
  hyphenated tokens don't fire; genuine subtraction typed as
  `5-3` would still be caught by asking the evaluator only when
  another heuristic also fires.
- Reject candidates matching a date shape (`\d{4}-\d{2}-\d{2}`)
  or ≥2 hyphens with no other operator.

Add `2024-01-15` and `555-1234` as explicit non-match tests
either way, so the decision is recorded.
