# Result ranking and ordering system

When multiple plugins return results for the same query, the
launcher needs a ranking system to produce a single ordered list.
This is one of the biggest UX differentiators — good ranking makes
the launcher feel like it reads your mind.

## Ranking signals

- **Fuzzy match score**: How well the query matches the result
  text. Primary signal for relevance.
- **Frecency** (frequency + recency): How often and how recently
  the user selected this result. The most important personalization
  signal. Alfred and Raycast both use this.
- **Plugin priority**: Some plugins are inherently higher priority
  (app launcher > web search fallback). Configurable per plugin.
- **Result type priority**: Commands > exact matches > fuzzy
  matches > fallback actions.
- **Prefix match bonus**: Results where the query matches the
  start of the title should rank higher than mid-word matches.
- **Contextual signals**: Time of day, active application, recent
  clipboard content? (Advanced, probably not v1.)

## Frecency implementation

Frecency combines how often an item is used with how recently:
- Each selection records a timestamp
- Score = sum of time-decayed weights for recent selections
- Decay function: recent uses count more, old uses fade out
- Storage: per-result-ID selection history (bounded, e.g. last
  30 selections)

## Interleaving strategy

- **Unified ranked list**: All results from all plugins merged
  into one list, sorted by combined score. Simple, clean.
- **Grouped with priority**: Plugin results grouped with section
  headers, ordered by plugin priority. Within each group, ranked
  by match score. More structured but less dynamic.
- **Hybrid**: Top results from each plugin interleaved by score,
  with a "more from X" grouping for lower-ranked results.

## Open questions

- Where does frecency data live? SQLite? JSON file? In-memory
  with periodic flush?
- How much history to keep per item? Trade-off between accuracy
  and storage.
- Should ranking be purely Rust-side, or can the frontend
  influence it (e.g. boosting visible results)?
- Can users manually pin/boost items to override ranking?
- How to handle new items with no history? Cold start problem.
