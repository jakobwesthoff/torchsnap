# Plugin: Open URL (with metadata enrichment)

Detect URLs in the search query and offer to open them in the default
browser, enriched with fetched page metadata (title, description,
favicon).

## Prerequisites

- Async plugin search architecture (channel-based concurrent search)
  must be in place. This plugin blocks its pool thread during HTTP
  fetches on cache misses — the architecture change makes that safe.

## Scope

### URL detection

- **Explicit scheme** (`https://example.com`, `http://...`): show
  result immediately on cache hit, fetch on miss.
- **Bare domain** (`example.com`, `github.com/foo`): only show a
  result after a successful fetch confirms the URL is reachable.
  Auto-prepend `https://`.
- Deep links (`slack://`, `vscode://`) deferred to a later iteration.

### Result type

Standard `QueryPlugin` returning `Results` entries in the normal
search list. Not InlineUI or CustomUI — this is a regular result that
participates in scoring and sorting alongside other plugins.

### Actions

- Enter: open URL in default browser via `tauri-plugin-opener`.

### Metadata extraction

Adapted from squirly's `metadata/mod.rs` and `html_fields.rs`. Lives
in this plugin's module for now (no shared crate extraction yet).

Extracted fields:
- **Title**: `og:title` → `<title>` fallback
- **Description**: `og:description` → `<meta name="description">`
  fallback
- **Favicon**: scored selection from `<link rel="icon">` elements
  (SVG > PNG > other > ICO, largest preferred), fallback to
  `/favicon.ico`

Two-phase streaming extraction from squirly:
1. Stream up to 256 KB or `</head>` / `<body`, attempt extraction.
   If all fields found, return early.
2. If fields missing, download remainder (up to 5 MB), re-extract.

Favicon image is downloaded and validated (`image/*` Content-Type,
magic bytes via `infer` crate, SVG substring check). Stored as binary
blob in the cache.

### HTTP client

Direct `reqwest` usage. No SSRF protection needed — this is a local
desktop app; the user already has the same network access through
their browser.

- Connect timeout: 5s
- Total timeout: 10s
- Max response size: 5 MB
- No idle connection pooling (unique domains)

### Cache

SQLite via existing `SqlStorage` infrastructure.

- **Cache key**: normalized URL (lowercase scheme/host, strip default
  ports, remove trailing slash, drop empty query/fragment). URL
  normalization logic adapted from squirly's `normalized_url.rs`.
- **Stored fields**: title, description, favicon_url, favicon image
  (blob), fetched_at timestamp, content_hash (blake3) for dedup.
- **TTL**: configurable in settings (default TBD — 7 or 30 days).
- **Retention cleanup**: background thread on plugin startup, same
  pattern as calculator history.

### Settings UI

Same pattern as calculator:
- Enable/disable toggle
- Cache TTL slider
- Stats display (entry count, DB size)
- "Clear All" button with confirmation

### Dependencies (new)

- `scraper` — HTML parsing with CSS selectors (for metadata
  extraction)
- `infer` — magic byte detection for favicon validation
- `reqwest` — HTTP client (may already be a transitive dependency)

### Not in scope for v1

- Deep links (`slack://`, `vscode://`)
- Favicon display in the result list (if the frontend result
  component doesn't support images yet — may need work)
- Streaming updates (show placeholder first, then enrich) — the
  architecture supports this later via multiple `ResultChannel`
  sends, but v1 waits for the full fetch before returning anything
- URL history / frecency boosting for previously opened URLs

## Reference

- Squirly metadata extraction: `crates/squirly/src/metadata/mod.rs`
- Squirly HTML field helpers: `crates/squirly/src/html_fields.rs`
- Squirly URL normalization: `crates/squirly/src/normalized_url.rs`
- Squirly favicon logic: inside `metadata/mod.rs` (scoring) and
  `favicons/mod.rs` (image download)
- Squirly URL detection hook:
  `packages/acornkit/src/launcher/hooks/useUrlDetection.ts`
- Original todo: `01kmh0dspar1gga0ahh1pyp353-plugin-open-url.md`
  (superseded by this file)
