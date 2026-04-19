# Convert emoji-picker to a WASM plugin

## Context

The emoji-picker is the native plugin closest to being WASM-ready. Every
capability it needs is either already exposed by the host WIT layer or can
live entirely inside the WASM binary itself:

- **Clipboard write** — `clipboard::write-text` already exists in the WIT.
- **Frecency storage** — the native plugin uses `PluginFrecency`; in WASM
  this maps cleanly to the `sql` host import (one table, simple upsert).
- **Fuzzy matching** — `nucleo_matcher` compiles to `wasm32-wasip2`; it can
  be an in-plugin dependency with no host involvement.
- **Emoji data** — currently `include_str!` from `OUT_DIR`; in the WASM
  plugin the build script embeds the same JSON blobs directly into the binary
  via `include_str!` / `include_bytes!` at compile time, or they can be
  inlined as `static` arrays. The data files live under
  `plugins/emoji-picker/data/` (to be created alongside the plugin crate).

No new WIT interfaces are required.

## Current State

- Native implementation: `src-tauri/src/plugins/emoji.rs`
- Emoji/shortcode data generated in `src-tauri/build.rs` from
  `src-tauri/assets/emoji/` JSON sources
- Frecency managed by `PluginFrecency` (host-side helper)
- Plugin registered in `src-tauri/src/plugins/mod.rs`

## Target

A new WASM plugin crate under `plugins/emoji-picker/` that:

1. Embeds emojibase `en/data.json` and the GitHub/emojibase shortcode JSONs
   at compile time (copy the existing asset files from `src-tauri/assets/emoji/`
   into the plugin crate; adjust `build.rs` or embed directly).
2. Implements two-pass nucleo fuzzy search (shortcode pass with score bonus,
   then label/tag pass) entirely in-WASM — port logic from `emoji.rs`.
3. Stores frecency in the plugin's SQL database:
   ```sql
   CREATE TABLE IF NOT EXISTS frecency (
       emoji   TEXT NOT NULL PRIMARY KEY,
       score   REAL NOT NULL DEFAULT 0.0,
       used_at INTEGER NOT NULL DEFAULT 0
   );
   ```
4. Prefix-activates on `:` and returns `search-response::results`.
5. `execute` for the select action calls `clipboard::write-text`.
6. Browse (empty/no-prefix query) returns entries ordered by frecency score
   descending.
7. Listed in `plugins/bundled.toml` and the native `emoji.rs` registration
   removed from `src-tauri/src/plugins/mod.rs`.

## Migration Notes

- The frecency scoring algorithm used by `PluginFrecency` should be
  replicated faithfully (or simplified) inside the WASM crate to avoid
  changing browse behaviour.
- The native plugin hard-codes a `:` prefix; replicate this in
  `manifest.toml` under `[[prefixes]]`.
- Since WASM plugins have no background threads, the full emoji list is
  loaded once in `lifecycle::enable` and kept in linear memory — no
  incremental loading needed given the data set size.

## Open Questions

- Should the frecency algorithm be an exact port of `PluginFrecency`, or
  is this an opportunity to simplify it?
- Do we want to preserve existing frecency scores from the native plugin
  on first run, or start fresh?

## References

- `src-tauri/src/plugins/emoji.rs` — native implementation to port
- `src-tauri/build.rs` — how emoji asset data is currently prepared
- `src-tauri/assets/emoji/` — JSON source files
- `plugins/calculator/` — reference WASM plugin structure
- `wit/torchsnap-plugin.wit` — `sql` and `clipboard` host imports
