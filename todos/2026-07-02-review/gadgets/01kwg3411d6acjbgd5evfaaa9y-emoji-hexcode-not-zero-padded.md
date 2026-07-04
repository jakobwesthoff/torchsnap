# Emoji picker: hexcode lookup is not zero-padded — 14 emoji are unreachable

**Kind:** bug
**Severity:** medium
**Area:** gadgets/emoji-picker/src/lib.rs

## Problem

Shortcode files from emojibase are keyed by *zero-padded* hexcode
(minimum 4 hex digits per codepoint: `00A9-FE0F`,
`0023-FE0F-20E3`). The gadget reconstructs the key from the emoji
string without padding (`gadgets/emoji-picker/src/lib.rs:184-190`):

```rust
let hexcode = entry
    .emoji
    .chars()
    .map(|c| format!("{:X}", c as u32))
    .collect::<Vec<_>>()
    .join("-");
```

`format!("{:X}", 0xA9)` yields `A9`, not `00A9`, so every emoji
containing a codepoint below `U+1000` builds a key that exists in
no shortcode file. The `-FE0F`-stripped fallback
(`lib.rs:190-195`) doesn't help — it has the same padding problem.

Verified against the actual embedded data
(`frontend/node_modules/emojibase-data/en/shortcodes/github.json`):
exactly 14 entries are affected — copyright (©), registered (®),
and the 12 keycaps (`#⃣ *⃣ 0⃣–9⃣`). Each ends up with
`shortcodes: []`.

Entries without shortcodes are then filtered out of *every*
result path: the empty-query browse grid
(`lib.rs:276-278,285-287`) and the final search assembly
(`lib.rs:414-417`). The 14 emoji are silently unreachable in the
picker — searching `copyright` or `keycap` returns nothing for
them even though their labels/tags would match in pass 2.

## Suggested fix

Pad each codepoint to 4 digits, matching emojibase's convention:

```rust
.map(|c| format!("{:04X}", c as u32))
```

(Codepoints above `U+FFFF` format wider than 4 digits in both
schemes, so `{:04X}` is exactly emojibase's format.) Add a
regression check that a known low-codepoint emoji (e.g. ©) ends
up with a non-empty shortcode list after `parse_emoji_data` —
the parse function is pure and testable natively.
