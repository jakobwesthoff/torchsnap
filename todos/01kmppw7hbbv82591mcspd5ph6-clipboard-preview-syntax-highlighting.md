# Clipboard: Syntax highlighting for text preview

**Priority: polish / nice-to-have**

The detail preview panel renders all text content as plain monospace.
For clipboard entries containing code, JSON, XML, or structured data,
syntax highlighting would significantly improve readability.

## Design considerations

- **Language detection**: No metadata about source app or content type
  is available. Would need heuristic detection (e.g., highlight.js
  `highlightAuto` or a lightweight alternative). Could also check
  for common patterns: JSON (starts with `{`/`[`), XML/HTML (starts
  with `<`), etc.
- **Performance**: Highlighting runs on every selection change. With
  the LRU cache we could cache highlighted output alongside the raw
  text, but the highlight itself should be fast for 1000-char
  snippets.
- **Library choice**: Evaluate shiki (tree-sitter based, accurate but
  heavier) vs highlight.js (regex-based, lighter) vs a minimal
  custom approach for just a few common formats.
- **Theme integration**: Highlighting colors should respect the app's
  theme tokens to stay visually consistent.
- **Fallback**: If detection confidence is low, render as plain text
  rather than showing incorrect highlighting.
