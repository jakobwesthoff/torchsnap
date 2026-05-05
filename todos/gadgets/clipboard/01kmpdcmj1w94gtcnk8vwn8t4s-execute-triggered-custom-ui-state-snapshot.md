# Execute-triggered custom UI: state snapshot and restore

When a catalog-mode gadget entry's `execute()` returns `PostAction::ShowCustomUI`,
the host should snapshot its current state (query, results, selected index)
before mounting the gadget's custom UI. When the user presses Escape /
`goBack()`, the snapshot is restored so the user returns to exactly where
they were.

## Current behavior (initial implementation)

Escape from an execute-triggered custom UI returns to an empty query
state. No snapshot is taken or restored. This is acceptable for the
clipboard manager (the entry is always findable) but loses context for
drill-in flows where the user was mid-search.

## Future implementation

- On `ShowCustomUI`: store `{ query, results, selectedIndex }` in a
  ref or state variable in `Launcher.tsx`
- On `goBack()`: restore from snapshot instead of clearing query
- Stack-based if nested drill-ins are ever needed (unlikely near-term)

## Dependencies

- `PostAction::ShowCustomUI` must be implemented first
- Related to ADR 0013 topic 1 (execute-triggered activation path)
