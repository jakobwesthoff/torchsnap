# Plugin: Web search with configurable engine

Fall-through action that opens a web search for the current query.

## Scope

- Always available as a low-priority result when the query doesn't
  match other plugins exactly
- Configurable search engine (Google, DuckDuckGo, Brave, custom URL
  with `%s` placeholder)
- Enter opens `<engine-url>?q=<query>` in default browser
- Support multiple engines with a prefix system (e.g. `g:` for
  Google, `ddg:` for DuckDuckGo)

## Settings

- Default search engine selection
- Custom engine URL template
- Prefix → engine mapping

## Implementation

- Minimal — mostly URL template substitution
- Should appear at the bottom of results as a fallback
- Icon: magnifying glass or search engine favicon
