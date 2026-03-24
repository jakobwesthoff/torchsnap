# Plugin: Emoji picker

Search and insert emoji by keyword — surprisingly high daily usage
for a launcher feature.

## Scope

- Search emoji by name, keyword, and aliases
- Show emoji with name in result rows
- Enter copies selected emoji to clipboard
- Recent/frequently used emoji section when query is empty
- Skin tone variant support (modifier selection)
- Trigger: could be always-on fuzzy match, or prefix-activated
  (e.g. `:` prefix like Slack)

## Implementation

- Embed the Unicode CLDR emoji data (names, keywords, categories)
  as a static dataset — ~3500 emoji, small memory footprint
- Fuzzy search across name + keywords
- Group by category when browsing (People, Nature, Food, etc.)
- Store usage frequency for "recently used" sorting

## Data source

- Unicode CLDR annotations: provides emoji names and keywords in
  multiple languages
- `unicode-emoji-data` npm package or equivalent Rust crate
- Or vendor a static JSON/binary blob at build time

## UX details

- Show the actual emoji glyph large enough to distinguish similar
  ones
- Show the official Unicode name as secondary text
- Skin tone: show default (yellow) by default, allow a modifier
  picker on long-press or secondary action
- Consider showing the emoji in the search input as a live preview
  while navigating results
