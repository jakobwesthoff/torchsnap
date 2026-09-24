---
kind: bug
severity: medium
status: open
area: [src-tauri/src/unicode.rs, src/lib/highlightText.tsx]
---

# Highlighting a multi-code-unit grapheme splits surrogate pairs (renders U+FFFD)

## Problem

`GraphemePositions::into_utf16`
(`src-tauri/src/unicode.rs:43-68`) maps each matched grapheme
index to the grapheme's *starting* UTF-16 offset only:

```rust
for pos in indices.iter_mut() {
    if let Some(&utf16_offset) = cluster_offsets.get(*pos as usize) {
        *pos = utf16_offset;
    }
}
```

A grapheme that is 2+ UTF-16 code units (emoji, combining-mark
sequences, flags) therefore contributes exactly one position, not
its full code-unit range. Contrast with
`Utf16Positions::from_substring` in the same file, which expands
matches to the full range (`unicode.rs:129-133`,
`utf16_start..utf16_end`).

The frontend consumer `highlightText`
(`src/lib/highlightText.tsx:31-49`) walks the string one UTF-16
code unit at a time (`text[i]`) and starts/ends `<span>`s when
set membership flips. With an emoji at units `k, k+1` and only
`k` in the position set:

- `i = k`: match run starts, `current` receives the **high
  surrogate** alone.
- `i = k+1`: membership flips to false, the span closes holding
  half a surrogate pair; the low surrogate starts the unmatched
  segment.

A lone surrogate in a DOM text node renders as U+FFFD, so the
entry title shows `��` (or a bare combining mark attached to
nothing for `e` + U+0301 sequences) exactly at the matched
character.

## Trigger

Any catalog fuzzy match where nucleo's `Pattern::indices` matches
a multi-code-unit grapheme — e.g. an app or catalog entry title
containing an emoji ("📝 Notes") when the user's query includes
that character, or matches on decomposed accented characters.
ASCII-only titles are unaffected (`into_utf16` short-circuits).

## Suggested fix

In `into_utf16`, expand each grapheme index to its full UTF-16
code-unit range (store `(start, len)` while scanning, push
`start..start+len`), mirroring `from_substring`'s behavior.
`highlightText` then keeps the whole pair inside one span with no
frontend change. Add a Rust test asserting the emoji case yields
both code units, and a frontend test that highlighting an emoji
position set does not split the pair.

Secondary observation in the same function: an out-of-range
grapheme index (no `cluster_offsets` entry) is silently left in
grapheme space (`unicode.rs:61-65`), mixing index spaces in the
output vector. Consider dropping such positions instead.
