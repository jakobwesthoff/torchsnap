# Convert bangs to a WASM plugin

## Context

The bangs plugin detects `!bang` tokens anywhere in a query (always-on, no
prefix) and resolves them against a local SQLite database populated from
DuckDuckGo's `bang.js`. It returns a single result that opens the bang URL
in the browser. Also supports a Copy-URL action and a settings UI for manual
refresh.

**Blocked by:** `http` + `opener` WIT interfaces
(`todos/wasm/*-wasm-host-http-opener-interfaces.md`).

## What maps to existing WIT

- SQLite database — `sql` host import ✅
- Clipboard copy — `clipboard::write-text` ✅
- Open URL in browser — `opener::open-url` (new interface, see prerequisite todo)
- Fetch fresh `bang.js` on first run / manual refresh — `http::fetch` (new interface)
- Baked-in `bang.js` fallback — embed as `include_bytes!` in the WASM binary

## What needs special attention

- **`bang.js` parsing**: the baked-in copy lives pre-parsed at
  `src-tauri/derived/bang.json`. The WASM plugin should embed this JSON blob
  and import it into SQLite at `lifecycle::enable` time (only if the DB is
  empty). The HTTP refresh path calls `http::fetch`, re-parses, and
  re-populates the DB.
- **`WebsiteMetadataService`** (favicon enrichment): the native plugin uses a
  shared in-process Rust service. Defer favicon enrichment until a
  `website-metadata` host import exists — ship without favicons initially.
- **Settings UI** for manual refresh: uses `messaging::handle-message` with
  a `refresh` method, same pattern as the calculator plugin. No new WIT
  needed.
- **Background refresh**: the native plugin refreshes periodically; replace
  the background thread with a `[[tasks]]` cron entry in `manifest.toml`.
- **`http` domain allowlist**: if the host enforces a per-manifest domain
  allowlist, bangs needs `duckduckgo.com` declared.

## Migration Steps

1. Implement the `http` + `opener` host interfaces (prerequisite todo).
2. Create `plugins/bangs/` crate mirroring the calculator structure.
3. Port logic from `src-tauri/src/plugins/bangs/mod.rs`.
4. Add to `plugins/bundled.toml`.
5. Remove native registration from `src-tauri/src/plugins/mod.rs`.

## Open Questions

- Favicon enrichment: defer entirely, or implement a lightweight fallback
  (e.g. `<domain>/favicon.ico` heuristic via `http::fetch`) without the
  full metadata service?
- Import strategy for the baked-in JSON: parse at `enable()` every time, or
  check if the DB is already populated and skip?

## References

- `src-tauri/src/plugins/bangs/mod.rs` — native implementation to port
- `src-tauri/derived/bang.json` — pre-parsed baked-in bang data
- `plugins/calculator/` — reference WASM plugin structure
- `wit/torchsnap-plugin.wit` — WIT world to extend
- Prerequisite: `todos/wasm/*-wasm-host-http-opener-interfaces.md`
