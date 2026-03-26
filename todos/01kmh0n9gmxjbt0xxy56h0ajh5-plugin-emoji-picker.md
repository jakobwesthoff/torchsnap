# Plugin: Emoji picker

Search and insert emoji by keyword — surprisingly high daily usage
for a launcher feature. Prefix-activated with `:` (colon), matching
the universal shortcode convention (Slack, Discord, GitHub).

## Decisions

- **Prefix**: `:` (colon) — strict activation, no emoji results
  without prefix
- **Data source**: emojibase (`emojibase-data` npm package), English
  locale. Install as bun dev dependency, vendor the JSON into the
  Tauri binary via `include_str!()`
- **Rendering**: native emoji font — no image assets, platform
  handles rendering. Matches what users paste into other apps.
- **Matching**: two-pass search with nucleo. First pass matches
  shortcodes, second pass matches keywords/tags. Shortcode matches
  always rank above keyword matches.
- **Skin tone**: show default (yellow) only for now. Variants
  deferred.
- **Action**: copy emoji to clipboard (primary action)
- **Display**: standard result list for now. Grid rendering deferred
  to plugin custom UI system (see plugin-custom-ui todo).
- **Recently used / frecency**: deferred to frecency tracking system
  (see result-ranking-system and emoji-frecency todos).

## Implementation

- Rust-side `CatalogPlugin` with `:` prefix
- `setup()`: parse emojibase JSON (embedded via `include_str!()`)
  into internal entry list. ~4,500 entries, sub-10ms parse time.
- `entries()`: return all emoji as `CatalogEntry` items with emoji
  glyph as title, shortcode as subtitle
- Two-pass nucleo matching: shortcode pass gets score boost over
  keyword pass
- Clipboard write via Tauri clipboard plugin

## Data shape from emojibase

Each entry provides:
- `emoji` — the Unicode character(s)
- `label` — descriptive name ("grinning face")
- `shortcodes` — array of shortcode strings
- `tags` — keyword array for search
- `group` / `subgroup` — category grouping
- `skins` — skin tone variants (deferred)

## UX details

- Show the actual emoji glyph large enough to distinguish similar
  ones (as icon in result row)
- Show shortcode as title, label as subtitle
- Consider showing the emoji in the search input as a live preview
  while navigating results (future enhancement)
