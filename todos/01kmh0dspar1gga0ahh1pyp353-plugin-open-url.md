# Plugin: Open URL

Detect URLs in the search query and offer to open them in the
default browser.

## Scope

- Detect when the query looks like a URL (starts with `http://`,
  `https://`, or matches a domain pattern like `example.com`)
- Show a single result item: "Open <url> in browser"
- Enter opens the URL via `tauri-plugin-opener`

## Reference

Nutty has `useUrlDetection` in acornkit that detects URLs and also
offers to add them as bookmarks (squirly-specific). The URL
detection logic can be reused, the bookmark part is not relevant.

## Implementation

- URL regex or heuristic matcher
- Show favicon if fetchable (optional, latency concern)
- Could also support deep links (e.g. `slack://`, `vscode://`)
