# Emoji picker: frecency for empty query

When only `:` is typed (empty query after prefix), the emoji grid
currently shows an unfiltered list of emoji in emojibase order. This
should eventually show frecency-ranked emoji — most recently and
frequently used first.

Depends on the result-ranking-system todo being implemented first.

Related: ADR 0013 (emoji grid is the test case for plugin custom UI).
