# Fix Nucleo Grapheme Index Mismatch

`title_positions` in `ScoredEntry` are nucleo grapheme indices. These only
equal JavaScript string indices for ASCII characters. For emoji, CJK, and
other multi-byte/multi-codepoint characters, highlight positions are wrong on
the frontend.

This is documented as a TODO in `search/types.rs:184`.

Options:
- Convert grapheme indices to byte/codepoint offsets in the Rust side before
  serialization
- Convert on the frontend side when rendering highlights
- Use a shared understanding of "character index" that both nucleo and JS
  agree on (UTF-16 code unit indices, matching JS string indexing)

The conversion should happen on the Rust side since that's where the grapheme
information is available.
