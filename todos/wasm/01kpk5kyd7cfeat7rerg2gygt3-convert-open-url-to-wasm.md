# Convert open-url to a WASM plugin

## Context

The open-url plugin is always-on and detects bare URLs or `http(s)://` URLs
in the search query. It shows a result to open or copy the URL, enriched
with page title and favicon via the native `WebsiteMetadataService`.

## What maps to existing WIT

- Clipboard copy — `clipboard::write-text` ✅
- Open URL in browser — `opener::open-url` ✅
- Page title + favicon — `torchsnap_plugin_sdk::website_metadata::lookup_cached`
  (or `favicon_or` for icon-only). The host service is exposed via the
  `website-metadata` WIT interface; gate with
  `[permissions]\nwebsite-metadata = true` in the manifest.

## What needs special attention

- **URL detection**: the native plugin uses the `url` and `addr` crates
  (Public Suffix List for bare-domain validation). Both compile to
  `wasm32-wasip2`; include as direct dependencies in the plugin crate.
- **Stateless**: no storage needed — purely query → result per invocation.
- **Lookup mode**: open-url runs per-keystroke, so `lookup_cached` is the
  right default. Falling back to a hero icon while the host warms the
  cache mirrors the bangs plugin's pattern.

## Migration Steps

1. Create `plugins/open-url/` crate mirroring the calculator structure.
2. Port logic from `src-tauri/src/plugins/open_url/mod.rs`.
3. Add to `plugins/bundled.toml`.
4. Remove native registration from `src-tauri/src/plugins/mod.rs`.

## References

- `src-tauri/src/plugins/open_url/mod.rs` — native implementation to port
- `plugins/calculator/` — reference WASM plugin structure
- `plugins/bangs/src/lib.rs` — recent reference port using `http`,
  `opener`, and `website_metadata::favicon_or`
- `plugins/plugin-sdk/wit/torchsnap-plugin.wit` — WIT world
