---
kind: bug
severity: low
status: open
area: [src/launcher/compareEntries.ts, src-tauri/src/commands/types.rs]
tags: [unconfirmed]
---

# Rust and TS sort comparators disagree on non-BMP strings despite MUST-stay-in-sync contract

## Problem

Both sides declare a hard sync contract on the result-sort
comparator (`src-tauri/src/commands/types.rs:198-204`,
`src/launcher/compareEntries.ts:7-15`: "MUST stay in sync").
The field order and directions match (score DESC, source ASC,
id ASC), but the *string comparison semantics* differ:

- Rust `str::cmp` (`commands/types.rs:205-212`) compares by
  UTF-8 bytes, which is equivalent to Unicode code point order.
- JS `<` / `>` (`compareEntries.ts:18-21`) compares by UTF-16
  code units.

These two orders disagree whenever two strings first differ at
a position where one has a BMP code point in `[U+E000, U+FFFF]`
(UTF-16 unit `0xE000-0xFFFF`) and the other has a supplementary
code point (first UTF-16 unit is a surrogate,
`0xD800-0xDFFF`). Example: `"\u{FDFD}"` vs `"\u{1F600}"` (😀) —
Rust sorts `U+FDFD` before `U+1F600` (code points
`0xFDFD < 0x1F600`); JS sorts `"😀"` first (units
`0xD83D < 0xFDFD`). The `id` tiebreak is the exposed field:
entry ids are arbitrary gadget-defined strings, and the
emoji-picker sets `id` to the emoji character itself
(`gadgets/emoji-picker/src/lib.rs:466`, confirmed by the
comment at `:560-561`), so non-BMP ids flow through this
comparator in production whenever equal-scored entries from the
same source are tie-broken.

## Impact

None user-visible today, for a specific reason: the frontend
never consumes the Rust ordering. `useSearch` re-sorts the full
flattened accumulator with its own comparator on every message
(`src/launcher/hooks/useSearch.ts:122-126`), so display order
is purely JS-defined.

The divergence becomes a live bug the moment the optimization
documented right above that sort is implemented
(`useSearch.ts:113-121`): a k-way merge across "already
pre-sorted" incoming batches assumes each batch is sorted under
the *same* comparator the merge uses. Rust-sorted batches
containing ids that straddle the diverging character class
would merge out of order, producing a subtly wrong display
order that only manifests with emoji/exotic ids.

## Suggested fix

Make the TS comparator match code point order explicitly, e.g.
compare with `Array.from(a.id)` code-point-wise or use
`a.id.localeCompare` — no: `localeCompare` is locale-dependent
and diverges further. The minimal faithful port is a manual
code-point iteration, or normalize the contract the other way
(make Rust compare `encode_utf16()` sequences). Either
direction is fine as long as both files change together and the
sync comment documents the chosen unit (code points vs UTF-16
units). Add a paired test on both sides with a fixture pair
from the diverging class (e.g. `"\u{FDFD}"` vs `"\u{1F600}"`)
so a future drift fails loudly.
