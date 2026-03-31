# Merge QueryResult into ScoredEntry

`QueryResult` and `ScoredEntry` are structurally identical minus the `source`
field. `QueryResult::into_scored_entry()` is seven lines of mechanical
field-copying.

Options:
- Make `ScoredEntry` a newtype: `struct ScoredEntry { source: String, inner: QueryResult }`
  with `#[serde(flatten)]`
- Eliminate `QueryResult` entirely and have query plugins produce `ScoredEntry`
  directly (the host can fill `source` via a builder or setter)
- Keep both but use a macro to derive one from the other

The goal is to eliminate the field-copying boilerplate and the risk of adding a
field to one but not the other.
