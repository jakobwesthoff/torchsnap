# Convert open-url to a WASM plugin

## Context

The open-url plugin is always-on and detects bare URLs or `http(s)://` URLs
in the search query. It shows a result to open or copy the URL, enriched
with page title and favicon via the native `WebsiteMetadataService`.

**Blocked by:** `http` + `opener` WIT interfaces
(`todos/wasm/*-wasm-host-http-opener-interfaces.md`).

## What maps to existing WIT

- Clipboard copy — `clipboard::write-text` ✅
- Open URL in browser — `opener::open-url` (new interface, see prerequisite todo)

## What needs special attention

- **`WebsiteMetadataService`**: this is the main design question beyond `http`
  and `opener`. The native plugin calls a shared in-process Rust service that
  fetches page title and favicon. Two options for the WASM port:
  - **Option A**: add a `website-metadata::fetch(url) → result<page-metadata,
    string>` host import wrapping the existing service. Cleaner for reuse
    across plugins (open-url and bangs both benefit).
  - **Option B**: use `http::fetch` directly in the plugin and implement
    lightweight title/favicon extraction in WASM — parse `<title>` from the
    HTML response, use the `<domain>/favicon.ico` heuristic. Avoids a new WIT
    interface but duplicates logic if other plugins need the same.
  Decide between options before starting the port.
- **URL detection**: the native plugin uses the `url` and `addr` crates
  (Public Suffix List for bare-domain validation). Both compile to
  `wasm32-wasip2`; include as direct dependencies in the plugin crate.
- **Stateless**: no storage needed — purely query → result per invocation.

## Migration Steps

1. Implement the `http` + `opener` host interfaces (prerequisite todo).
2. Decide on Option A vs B for metadata/favicon enrichment (see above).
3. Create `plugins/open-url/` crate mirroring the calculator structure.
4. Port logic from `src-tauri/src/plugins/open_url/mod.rs`.
5. Add to `plugins/bundled.toml`.
6. Remove native registration from `src-tauri/src/plugins/mod.rs`.

## Open Questions

- **Metadata enrichment strategy**: Option A (dedicated `website-metadata`
  host import) or Option B (raw `http::fetch` + in-WASM HTML parsing)?
- If Option A: should the `website-metadata` interface be added as part of
  this todo or as its own prerequisite todo?

## References

- `src-tauri/src/plugins/open_url/mod.rs` — native implementation to port
- `plugins/calculator/` — reference WASM plugin structure
- `wit/torchsnap-plugin.wit` — WIT world to extend
- Prerequisite: `todos/wasm/*-wasm-host-http-opener-interfaces.md`
