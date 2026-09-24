---
kind: improvement
status: needs-discussion
---

# Clipboard search cannot match inside a word

Analysed, options measured below, not yet decided.

Scope: `src-tauri/src/gadgets/clipboard/`. Requires a schema migration
and a full reindex of `clipboard_fts`. Land together with
[index coverage](01m1kchb9ya2hn091nyr0zkbj1-clipboard-fts-index-coverage.md),
which also rebuilds the index — doing both in one migration avoids
reindexing the user's history twice.

## Problem

`clipboard_fts` uses FTS5's default `unicode61` tokenizer
(`schema.rs:60-64`, no `tokenize=` argument). It indexes whole tokens,
so a query matches only at a token *prefix*. Searching `board` does not
find `ClipboardManager`; `clip` does.

Verified against the live history snapshot and asserted in
`storage.rs::search_matches_token_prefixes_but_not_infixes`, which
currently documents the limitation rather than fixing it.

This is the most likely remaining cause of a user reporting "search
missed my entry", now that the FTS5 escaping bug is fixed on branch
`clipboard-search-escaping`. Typing a remembered fragment from the
middle of a word is a natural habit, and the failure is silent: zero
results, no indication that the fragment could never have matched.

## Severity

Moderate, and more user-visible than the index-coverage gap because it
affects every entry type rather than files and markup only. It is not a
correctness bug — prefix search behaves exactly as FTS5 documents — so
it is a capability gap, and the fix carries real trade-offs (below)
rather than being a strict improvement.

## Option: trigram tokenizer

FTS5's `trigram` tokenizer supports substring matching. Confirmed
available in the SQLite this project actually ships — rusqlite 0.40
with the `bundled` feature (`src-tauri/Cargo.toml:42`), SQLite 3.53.2 —
by probing through `SqlStorage` rather than the system `sqlite3` CLI,
which is a different build. `MATCH 'board'` against `ClipboardManager`
returned a hit.

Measured trade-offs (all verified, not assumed):

| property | `unicode61` (today) | `trigram` |
| --- | --- | --- |
| infix match (`board`) | no | yes |
| queries under 3 characters | match as prefix | **no results at all** |
| case-insensitive | yes | yes |
| diacritic folding (`munchen` → `München`) | yes | only with `remove_diacritics 1` |
| index size on live corpus | 296 KB | 632 KB (~2.1x) |

Size was measured by building both indexes over the real 400-entry
corpus (162,514 characters of display text). At this scale ~340 KB of
growth is irrelevant; it is recorded so the ratio is known if history
sizes grow or retention is extended well beyond the 30-day default.

Two findings matter more than size:

1. **Short queries regress.** A 1- or 2-character query returns nothing
   under trigram, where today it prefix-matches. Since the launcher
   searches incrementally on every keystroke, a naive swap makes the
   list go blank for the first two characters and then populate — worse
   than the current behaviour for the common case of typing a word from
   the start.
2. **Diacritics need opting back in.** `remove_diacritics 1` restores
   the folding `unicode61` gives for free. Verified: without it,
   `munch` does not match `München`; with it, it does. Whatever is
   chosen must be covered by a test, because losing it is silent.

## Proposed design

Do not swap tokenizers wholesale. Two candidates:

**A. Dual index.** Keep the `unicode61` table for prefix matching and
add a parallel trigram table; query the trigram one only for terms of
three or more characters, union the results. Best behaviour, highest
cost: two indexes to keep in sync through the triggers, and the
existing trigger set is already a hazard (see the UPDATE-trigger
caveat in the companion todo).

**B. Trigram with a prefix fallback.** Single trigram index; for terms
under three characters, fall back to a `LIKE 'term%'` scan over
`clipboard_display`. Simpler to keep consistent. The fallback scan is
acceptable only because the corpus is small and bounded by retention —
at 400 rows it is trivial, and it runs only for the first two
keystrokes.

Recommendation: start from B and measure before considering A. The
extra index in A buys correct ranking and prefix semantics for short
terms, which recency ordering makes largely irrelevant — results are
already ordered by `captured_at`, not relevance.

Either way `build_fts_query` (`search.rs`) needs to branch on token
length, and its contract changes: today every token is quoted and the
last gets `*`. Under trigram the `*` is meaningless and quoting rules
differ, so the function grows a tokenizer-aware mode rather than being
edited in place.

## Test coverage

Extend the existing suites (20 unit tests in `search.rs`, 18
integration tests in `storage.rs`).

Default cases:

- `board` finds `ClipboardManager` (the assertion currently inverted in
  `search_matches_token_prefixes_but_not_infixes` — flip it and rename).
- Prefix search keeps working: `clip` still finds `ClipboardManager`.
- A mid-string fragment of a URL, a path, and a UUID each match.

Edge cases, one per measured trade-off above:

- 1- and 2-character queries return results (guards the regression that
  makes this fix worse than the status quo).
- Exactly 3 characters, the trigram boundary.
- `munchen` matches `München` (guards silent loss of diacritic folding).
- Case-insensitivity survives the tokenizer change.
- Every punctuation case from the escaping work still passes — the
  quoting rules change under trigram, so those 20 tests must be re-run
  against the new mode, not assumed to carry over.
- Empty and separator-only input still yields the unfiltered history.

Migration tests:

- Pre-migration database with known content; migrate; assert previously
  prefix-matchable terms still match and infix terms now match too.
- FTS row count matches `clipboard_display` after the rebuild.

Performance check (not a unit test): time a search over a synthetic
history an order of magnitude larger than the live one, to confirm the
short-query fallback in option B does not become the bottleneck.

## Documentation

- `CHANGELOG.md` entry.
- Update the `search.rs` module header, which currently explains
  escaping for a prefix tokenizer.
- Update the schema doc comment (`schema.rs:26-32`) with the tokenizer
  choice and the `remove_diacritics` setting.
- An ADR is warranted here if option A is chosen, since a dual index is
  a structural commitment; option B does not need one.
