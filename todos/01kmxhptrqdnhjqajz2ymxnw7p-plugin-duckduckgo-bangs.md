# Plugin: DuckDuckGo Bang Commands

Dedicated plugin (or extension of the web-search plugin) that allows users to
quickly trigger DuckDuckGo [bang commands](https://duckduckgo.com/bangs) — e.g.
`!yt cats` opens a YouTube search, `!g rust async` opens Google, `!gh torchsnap`
opens GitHub search.

## Motivation

Bang commands are a power-user shortcut that DuckDuckGo itself supports via
redirect (typing `!yt foo` at `duckduckgo.com` redirects straight to YouTube).
Surfacing them natively in Torchsnap would let users bypass the browser URL bar
entirely.

## Possible approaches

### 1. Pass-through to DuckDuckGo redirect

Simply construct `https://duckduckgo.com/?q=!yt+cats` and open it. DDG handles
the redirect. Zero maintenance — bangs are always up-to-date. Requires an
internet round-trip through DDG even when the bang destination is known.

### 2. Local bang database

Download / bundle the [public bang list](https://duckduckgo.com/bang.js) (~13k
entries) and resolve the redirect locally. Faster, works offline, but needs a
refresh strategy when bangs change.

### 3. Hybrid

Try local resolution first; fall back to DDG redirect if the bang is unknown.

## UX sketch

- User types `!yt lofi hip hop` → plugin surfaces a single result "Search YouTube
  for 'lofi hip hop'" with the YouTube favicon.
- User types `!` → show a fuzzy-searchable list of popular/recently used bangs.
- Could integrate with frecency: surface the most-used bangs at the top when `!`
  is typed alone.
- Consider whether this lives inside the existing web-search plugin or as its
  own plugin with its own query prefix (`!`).

## Open questions

- Overlap with `01kmh0dspar1gga0ahh1pyp354-plugin-web-search.md`: should bangs
  be a first-class feature of that plugin, or a separate plugin?
- How to handle bang disambiguation when multiple bangs share a prefix?
- Should we show bang descriptions/categories in the result list?
- Refresh cadence for a local bang database (if approach 2/3 is chosen).
