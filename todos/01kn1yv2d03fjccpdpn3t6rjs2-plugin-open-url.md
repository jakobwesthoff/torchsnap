# Plugin: Open URL (with metadata enrichment)

Detect URLs in the search query and offer to open them in the default
browser, enriched with fetched page metadata (title, description, favicon).

Metadata extraction logic adapted from the squirly project at
`../squirly/crates/squirly/src/`.

## Design Decisions (settled)

- **URL parsing**: `url` crate (already transitive dep) for scheme/host extraction
- **TLD validation**: `addr` crate — `parse_domain_name(host).has_known_suffix()`
- **Explicit scheme** (`https://...`): always show — enriched or globe-alt fallback
- **Bare domain** (`example.com`): validate TLD first, only show if reachable
- **Download cap**: 512 KB combined (phase 1: 256 KB or `</head>`/`<body`, phase 2: remainder)
- **Favicon**: fetch, validate, store as blob in SQLite, transfer as base64 `DataUrl`, render in result list
- **Cache TTL**: 30 days default, configurable
- **Default icon**: `globe-alt` heroicon for reachable-but-no-metadata or unreachable explicit URLs
- **Error handling**: `thiserror` for module-internal typed errors; `anyhow` at the Plugin trait boundary
- **New deps**: `url`, `addr`, `scraper`, `infer`, `reqwest` (with `rustls-tls` + `stream`), `base64`, `percent-encoding`, `thiserror`

## File Structure

```
src-tauri/src/plugins/open_url/
├── mod.rs              — plugin struct, URL detection, Plugin trait impl
├── html_fields.rs      — copy from squirly, title/description extraction
├── normalized_url.rs   — copy from squirly (verbatim, uses thiserror)
├── metadata.rs         — adapted from squirly, favicon scoring + extraction
├── http.rs             — new: reqwest streaming fetcher replacing squirly's Fetcher
└── cache.rs            — new: SQLite cache for metadata + favicon blobs

src/plugins/open-url/
└── OpenUrlSettings.tsx  — settings UI component
```

## Phase 1: Add Dependencies

**File:** `src-tauri/Cargo.toml`

```toml
url = "2"
addr = "0.15"
scraper = "0.22"
infer = "0.16"
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "stream"] }
base64 = "0.22"
percent-encoding = "2"
thiserror = "2"
```

`thiserror` enables typed error enums matching the squirly source style.
Use `thiserror` for module-internal error types (`NormalizedUrlError`,
`MetadataError`, `FetchError`). The plugin boundary still uses `anyhow`
for the `Plugin` trait methods that return `anyhow::Result`.

## Phase 2: Extract and Adapt Squirly Components

### 2a: `html_fields.rs` — copy verbatim

**Source:** `../squirly/crates/squirly/src/html_fields.rs` (217 lines)

Self-contained, depends only on `scraper`. Copy the entire file including
tests. Add MPL header. Change no logic.

### 2b: `normalized_url.rs` — copy verbatim

**Source:** `../squirly/crates/squirly/src/normalized_url.rs` (280 lines)

Copy as-is — `thiserror` is available so `NormalizedUrlError` stays
unchanged. Add MPL header. The struct, `parse()`, `host()`, `Display`,
`AsRef<str>`, and all tests transfer unchanged.

### 2c: `metadata.rs` — adapt from squirly

**Source:** `../squirly/crates/squirly/src/metadata/mod.rs` (1177 lines)

**Copy verbatim** (pure functions):
- `PageMetadata` struct
- `extract_metadata(html: &str, base_url: &Url) -> PageMetadata`
- All favicon types: `FaviconFormat`, `FaviconSize`, `FaviconRel`, `FaviconCandidate`
- All favicon functions: `parse_favicon_rel`, `detect_favicon_format`,
  `format_from_extension`, `parse_favicon_sizes`, `collect_favicon_candidates`,
  `score_favicon`, `select_best_favicon`, `extract_favicon`
- `decode_data_uri()`, `is_likely_svg()`
- Associated tests

**Remove** (replaced by `http.rs` and plugin orchestration):
- `Metadata` struct, `FaviconImageData`
- `fetch()`, `fetch_favicon_image()`, `extract_from_html()`

**Keep and adapt** `MetadataError` — stays as a `thiserror` enum but remove
the `Fetch` and `StoreFavicon` variants (squirly-specific). Keep `NotHtml`
and `NotImage`. Add variants as needed for the HTTP layer.

**Add**:
```rust
pub struct FaviconData {
    pub image_bytes: Vec<u8>,
    pub content_type: String,
}
```

**Adapt**: `use crate::html_fields` -> `use super::html_fields`

## Phase 3: HTTP Streaming Layer

**New file:** `src-tauri/src/plugins/open_url/http.rs`

Replaces squirly's `Fetcher`/`Response` with direct `reqwest`. All async
code wrapped in `tauri::async_runtime::block_on()` since `search()` runs
in `spawn_blocking` (same pattern as `settings_notifier.rs:73`).

**Shared client** created once in `setup()`, stored in plugin struct. Config:
- Connect timeout: 5s, total timeout: 10s
- User-Agent: `"TorchSnap/1.0"`
- Redirect limit: 5
- No idle connection pooling (disabled via pool config)

Define a `FetchError` enum with `thiserror` for typed HTTP errors
(`NotHtml`, `TooLarge`, `RequestFailed`, etc.). Functions return
`Result<T, FetchError>` internally; the plugin's `search()` matches on
specific variants to decide the three-state response.

### `fetch_page_metadata(client, url) -> Result<PageMetadata, FetchError>`

1. GET request, check `Content-Type` contains `text/html` (else bail)
2. **Phase 1**: Stream chunks into buffer. After each chunk, check for
   `</head>` or `<body` markers (lowercase), or `buffer.len() >= 256KB`.
   Stop on first match.
3. Try `extract_metadata()`. If title + description + favicon_url all
   present, return early.
4. **Phase 2**: Continue streaming up to 512KB combined. Re-extract.

Constants:
```rust
const EARLY_CHECK_SIZE: usize = 256 * 1024;
const MAX_DOWNLOAD_SIZE: usize = 512 * 1024;
```

### `fetch_favicon(client, favicon_url) -> Result<FaviconData, FetchError>`

1. If URL starts with `data:` -> use `decode_data_uri()` from metadata.rs
2. Otherwise GET with same client, cap response at 256 KB
3. Validate image: Content-Type `image/*` -> `infer::get()` magic bytes -> `is_likely_svg()`
4. Return `FaviconData { image_bytes, content_type }`

## Phase 4: SQLite Cache Layer

**New file:** `src-tauri/src/plugins/open_url/cache.rs`

Uses `SqlStorage` exactly like calculator.

### Schema

```sql
CREATE TABLE url_cache (
    normalized_url  TEXT PRIMARY KEY,
    title           TEXT,
    description     TEXT,
    favicon_blob    BLOB,
    favicon_type    TEXT,
    fetched_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);
CREATE INDEX idx_url_cache_fetched_at ON url_cache(fetched_at);
```

A row with null title/description/favicon = "reachable but no metadata"
(globe-alt case).

### Functions

- `cache_lookup(db, normalized_url, ttl_days) -> Option<CachedMetadata>`
- `cache_store(db, normalized_url, metadata)`
- `cache_clear(db) -> usize`
- `cache_stats(db) -> (entry_count, total_favicon_bytes)`
- `cache_evict_expired(db, ttl_days)`
- `favicon_to_data_url(blob, mime_type) -> String` (base64 helper)

### `CachedMetadata`

```rust
struct CachedMetadata {
    title: Option<String>,
    description: Option<String>,
    favicon_blob: Option<Vec<u8>>,
    favicon_type: Option<String>,
}
```

## Phase 5: Plugin Implementation

**File:** `src-tauri/src/plugins/open_url/mod.rs`

### URL Detection

```rust
fn detect_url(query: &str) -> Option<(String, bool)>
//                                    url     is_explicit_scheme
```

1. **Explicit scheme**: `url::Url::parse(query)` — if valid with host
   -> `Some((url, true))`
2. **Bare domain**: `url::Url::parse(&format!("https://{query}"))`,
   extract host, feed to `addr::parse_domain_name(host)`, check
   `.has_known_suffix()` -> `Some((url, false))`

### Plugin Struct

```rust
pub struct OpenUrlPlugin {
    enabled: Arc<AtomicBool>,
    db: Mutex<Option<Arc<SqlStorage>>>,
    client: Mutex<Option<reqwest::Client>>,
    cache_ttl_days: Arc<AtomicU32>,
    retention_condvar: Arc<Condvar>,
    retention_shutdown: Arc<Mutex<bool>>,
}
```

### `search()` — Three-State Logic

```
detect_url(query)?
  -> normalize URL for cache key
  -> check cache (return enriched result on hit)
  -> check cancellation
  -> fetch_page_metadata()
    -> OK: fetch_favicon(), cache result, send enriched entry
    -> OK but no metadata: cache minimal entry, send with globe-alt
    -> Err + explicit scheme: send with globe-alt, do NOT cache failure
    -> Err + bare domain: return silently (don't show)
```

Check `cancel.is_cancelled()` between URL detection and fetch, and
between page fetch and favicon fetch.

### `execute()`

- `ActionId::Open` -> `app.opener().open_url(entry_id, None)` -> `PostAction::Dismiss`
- `ActionId::Copy` -> `app.clipboard().write_text(entry_id)` -> `PostAction::Dismiss`

The entry ID is the URL string itself.

### `handle_message()`

- `"stats"` -> `{ entryCount, totalFaviconBytes }`
- `"clear_cache"` -> `cache_clear()`, returns `{ cleared }`

## Phase 6: Settings UI

**New file:** `src/plugins/open-url/OpenUrlSettings.tsx`

Same pattern as calculator settings:
- Enable/disable toggle (`usePluginSetting<boolean>("enabled")`)
- Cache TTL slider (`usePluginSetting<number>("cacheTtlDays")`, range 1-90)
- Stats display (entry count, favicon storage) via `sendPluginMessage("stats")`
- Clear cache button with confirmation via `sendPluginMessage("clear_cache")`

## Phase 7: Registration Wiring

1. **`src-tauri/src/plugins/mod.rs`**: Add `pub mod open_url;`
2. **`src-tauri/src/lib.rs`**: Add `host.register(Box::new(plugins::open_url::OpenUrlPlugin::new()));`
3. **`src-tauri/src/search/types.rs:106`**: Remove `#[allow(dead_code)]` from `EntryIcon::DataUrl`
4. **`src/plugins/registry.ts`**: Add `"open-url"` entry with `GlobeAltIcon` and settings component

## Phase 8: Verification

### Unit tests (in-module)
- `normalized_url.rs`: All squirly tests copied over
- `html_fields.rs`: All squirly tests copied over
- `metadata.rs`: Favicon scoring, `decode_data_uri`, `is_likely_svg`, `extract_metadata`
- `cache.rs`: Store/retrieve, TTL expiration, clear, stats (using `tempfile::tempdir()`)
- `mod.rs`: `detect_url` — explicit URLs, bare domains, invalid TLDs, paths, spaces

### Manual integration tests
- `https://github.com` -> enriched result with favicon and title
- `github.com` (bare) -> same enriched result
- `https://nonexistent.invalid` -> globe-alt fallback shown
- `nonexistent.invalid` (bare) -> no result
- Enter -> opens in browser
- Second query for same URL -> instant cached result
- Settings: toggle, TTL slider, stats, clear cache

## Key Reference Files

| What | Path |
|------|------|
| Plugin trait | `src-tauri/src/plugins/mod.rs:107` |
| Calculator (template) | `src-tauri/src/plugins/calculator.rs` |
| ResultChannel / EntryIcon | `src-tauri/src/search/types.rs:101-313` |
| SqlStorage | `src-tauri/src/storage/sql_storage.rs:246` |
| Plugin registration | `src-tauri/src/lib.rs:433` |
| Frontend icon rendering | `src/launcher/ResultRow.tsx:57` |
| Plugin registry (frontend) | `src/plugins/registry.ts` |
| Calculator settings (template) | `src/plugins/calculator/CalculatorSettings.tsx` |
| Squirly metadata | `../squirly/crates/squirly/src/metadata/mod.rs` |
| Squirly html_fields | `../squirly/crates/squirly/src/html_fields.rs` |
| Squirly normalized_url | `../squirly/crates/squirly/src/normalized_url.rs` |

## Not in scope for v1

- Deep links (`slack://`, `vscode://`)
- Streaming updates (show placeholder first, then enrich) — the
  architecture supports this later via multiple `ResultChannel` sends,
  but v1 waits for the full fetch before returning anything
- URL history / frecency boosting for previously opened URLs
