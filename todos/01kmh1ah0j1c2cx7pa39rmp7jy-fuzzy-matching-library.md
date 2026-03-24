# Fuzzy matching library

Every search-providing plugin needs fuzzy matching. This should be
a shared, host-provided capability rather than each plugin rolling
its own with inconsistent ranking and behavior.

## Library candidates

- **nucleo** (from helix editor): Best-in-class Rust fuzzy matcher.
  UTF-8 aware, very fast, supports match highlighting. Used in
  production by helix. Async/streaming API.
- **skim** (from fzf-like tool): fzf-compatible algorithm. Good
  but less actively maintained than nucleo.
- **fuzzy-matcher**: Simple, lightweight. Less sophisticated ranking.
- **sublime_fuzzy**: Sublime Text-style matching. Decent but older.

`nucleo` is the strongest candidate given its performance, active
maintenance, and match highlighting support.

## Plugin configurability

Not all plugins want the same fuzzy behavior. The host should
provide fuzzy matching as a service that plugins can configure:

- **Opt-out**: Some plugins may handle their own matching entirely
  (e.g. calculator just parses the expression, web search passes
  the raw query to an API). They should be able to disable host
  fuzzy matching for their results.
- **Match targets**: A plugin returning results with title,
  subtitle, and keywords should be able to specify which fields
  are matchable and their relative weight.
- **Minimum score threshold**: Plugins may want stricter or looser
  matching (an app launcher wants loose matching, a contact search
  might want stricter to avoid false positives).
- **Pre-filtered results**: Some plugins may do their own filtering
  (e.g. a database query) and return already-matched results that
  just need ranking/interleaving with other plugin results.

## Match highlighting

The fuzzy matcher should return match positions so the UI can
highlight matched characters in result rows. This is important
for the user to understand why a result appeared.

## Open questions

- Is fuzzy matching done on the Rust side (fast, works for WASM
  plugins) or JS side (easier for React rendering)?
- If Rust-side, how are match results + highlight positions passed
  to the frontend?
- Should the matcher run in a background thread to avoid blocking
  the main thread during large result sets?
