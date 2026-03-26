# Emoji frecency tracking

Track which emoji the user picks most often so that typing just `:`
(with no further query) shows a "recently/frequently used" section.

## Context

This is a specific application of the general frecency system
described in the result-ranking-system todo. Emoji is the clearest
use case because:
- The corpus is large (~4,500 entries) — without frecency, an empty
  `:` query would show an arbitrary alphabetical/group ordering
- Users have strong personal emoji preferences — a small set of
  ~20-30 emoji covers 90% of usage
- Other launchers (Raycast, Alfred) surface recent emoji prominently

## Requirements

- Record each emoji selection with a timestamp
- Compute a frecency score (frequency weighted by recency)
- When query is just `:` with no further text, show top-N frecent
  emoji instead of the full corpus
- Storage: reuse whatever persistence the general frecency system
  uses (likely SQLite or JSON file)

## Depends on

- plugin-emoji-picker — the plugin itself
- result-ranking-system — the general frecency infrastructure
