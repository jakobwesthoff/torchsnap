# Clipboard FTS index misses file paths and markup-only entries

Status: open — analysed, design sketched below, not yet decided.

Scope: `src-tauri/src/gadgets/clipboard/`. Requires a schema migration
and a full reindex of `clipboard_fts`. Land together with
[substring search](01m1kchb9ya2hn091nyr0zkbj2-clipboard-substring-search.md),
which also rebuilds the index — doing both in one migration avoids
reindexing the user's history twice.

Related work already done (branch `clipboard-search-escaping`): FTS5
query escaping, recency ordering, and a search error state. Those fixed
how the query is *built*; this todo is about what the index *contains*.

## Problem

`clipboard_display.display_text` serves two consumers with opposite
requirements, and the searchable one loses:

- The launcher list row, which truncates to `LIST_DISPLAY_MAX_CHARS = 40`
  (`schema.rs:87`) and wants something short and human-readable.
- `clipboard_fts`, an external-content FTS5 table whose only content
  source is `clipboard_display` (`schema.rs:60-64`), which wants the
  complete text.

`derive_display_text` (`formats.rs:183`) optimises for the first, so
three categories of entry are partly or wholly unsearchable.

### 1. File entries index basenames only

`file_paths_to_display_text` (`formats.rs:227`) maps every path through
`Path::file_name()`. Its doc comment states the intent plainly: "Uses
filenames (not full paths) to keep the display compact." Correct for a
40-character row, wrong for the index.

Measured on the live history (400 entries, 3 with a `files` format,
snapshot taken 2026-09-03):

| indexed (searchable)                     | stored in `clipboard_content` |
| ---------------------------------------- | ----------------------------- |
| `chatgpt`                                | `/Users/jakob/Desktop/maniac_mansion_plate/images/chatgpt` |
| `product_cleaned.jpg`                    | `/Users/jakob/Library/Containers/com.apple.MobileSMS/Data/tmp/TemporaryItems/com.apple.MobileSMS/LinkedFiles/89F22C5E-.../product_cleaned.jpg` |
| `support_FRITZ.Box_6690_Cable_...txt`    | `/Users/jakob/Downloads/support_FRITZ.Box_6690_Cable_...txt` |

Searching `Downloads` returns 0 hits despite one entry being a file
from `~/Downloads`.

The data is not lost. Full paths are stored as a JSON array in
`clipboard_content` under the `files` format (written by `store_entry`,
`storage.rs:283-291`; read back on paste by
`captured_to_clipboard_content`, `formats.rs:279-283`). Only the index
never sees them, so this is fixable by backfill without re-capturing.

Checked and ruled out: the sibling `text` format does **not** rescue
this. macOS often offers file paths as plain text too, but on the live
data the `text` flavour holds the basename again (`chatgpt`,
`support_FRITZ.Box_...txt`), not the path. There is no accidental back
door making these searchable.

### 2. HTML/RTF-only entries index nothing

`derive_display_text` returns `String::new()` when there is no files,
text, or image content (`formats.rs:206`) — the doc comment notes HTML
and RTF "don't produce display text (they're markup, not readable
content)". Such an entry gets an empty FTS document and can never match
any query. In practice most rich-text copies also carry a `text`
flavour, so this is the rarest of the three; it was not observed on the
live 400-entry history and is a correctness gap rather than a measured
user-facing loss.

### 3. Images index only their dimensions

Images become `Image (1920×1080)` (`formats.rs:203`), searchable only by
the literal word "image" or the dimensions. Arguably correct — there is
no text to index without OCR — but worth stating so it is a decision
rather than an oversight. No change proposed.

## Severity

Moderate and silent. No error is raised; the search simply returns
fewer rows than it should, which is indistinguishable from "I never
copied that". File entries are a small share of a typical history (3 of
400 here), so this is less severe than the escaping bug already fixed,
but it degrades exactly the case where search matters most: finding a
file copied days ago whose name you half-remember but whose directory
you remember exactly.

## Proposed design

Split the two jobs. Keep `display_text` exactly as it is for rendering
and give the index its own column:

```sql
ALTER TABLE clipboard_display ADD COLUMN index_text TEXT NOT NULL DEFAULT '';
-- clipboard_fts is rebuilt over (display_text, index_text), or over
-- index_text alone if display_text adds nothing the other lacks.
```

`index_text` is derived at capture from all formats rather than the one
winning format:

- files: the full paths, plus the basenames (basenames are already a
  substring of the path, so appending them is redundant unless the
  tokenizer changes — decide alongside the trigram question).
- html / rtf: a plaintext projection (tag-stripped) instead of `""`.
- text: unchanged.
- images: unchanged (`Image (w×h)`).

Open questions to settle before implementing:

1. One FTS column or two? Two columns allow weighting display text
   above path noise, but with recency ordering now in place
   (`ORDER BY e.captured_at DESC`) bm25 weights are unused, so a single
   concatenated column is probably sufficient. Prefer the simpler one
   unless relevance ordering returns.
2. Cap `index_text` separately? `DISPLAY_TEXT_MAX_CHARS = 128_000`
   (`schema.rs:84`) currently bounds both roles. A long path list could
   want a different bound than a long text paste.
3. HTML-to-plaintext: a dependency, or a minimal tag stripper? Prefer
   minimal — this feeds an index, not a renderer, and imperfect
   stripping degrades gracefully.

### Migration and backfill

Existing rows must be rebuilt, and the data needed is already present:

- files: parse the JSON array from `clipboard_content` where
  `format = 'files'`.
- html/rtf: re-derive from the stored bytes.
- text: copy `display_text`.

Then repopulate the FTS table (`INSERT INTO clipboard_fts(clipboard_fts)
VALUES('rebuild')`). Sizing on the live corpus: 400 entries, 162,514
characters of display text, so a rebuild is a sub-second operation at
realistic history sizes. Retention is 30 days by default
(`gadgets.clipboard-manager.retentionDays` in settings.json), which
bounds growth.

### Trigger caveat

`clipboard_display` has INSERT and DELETE triggers only
(`schema.rs:66-74`); there is no UPDATE trigger. This is currently safe
because display rows are only ever inserted and the dedupe path bumps
`captured_at` on `clipboard_entries` without touching display text
(`storage.rs:271-279`). **If the backfill UPDATEs `clipboard_display`
in place, the FTS index will silently go stale.** Either add an UPDATE
trigger as part of the migration or rebuild the FTS table explicitly
after the backfill. This is the single easiest thing to get wrong here.

## Test coverage

Extend the suite added in `storage.rs` (currently 18 integration tests
against a real SQLite database with the production schema) and
`search.rs` (20 unit tests).

Default cases:

- A file entry is found by a directory component (`Downloads`), by an
  intermediate component (`maniac_mansion_plate`), and by basename.
- A multi-file entry is found by a component of any one of its paths.
- An HTML-only entry is found by words in its text content.
- An RTF-only entry likewise.
- `display_text` is unchanged by all of the above — assert the list row
  still renders the compact basename, so the split is real.

Edge cases:

- A path component that is also an FTS5 keyword (`.../and/...`).
- Paths with spaces, unicode, and diacritics in directory names.
- An entry whose path list is long enough to hit the `index_text` cap.
- HTML whose tags contain words absent from the rendered text
  (`<div class="invoice">`) — decide and assert whether attribute text
  is indexed; recommendation is no.
- An image entry still matches `image` and nothing new.

Migration tests:

- Open a database at the pre-migration schema populated with file,
  html-only, and text entries; migrate; assert all become searchable by
  their new terms. This is the test that catches a backfill that
  forgets to rebuild the index.
- Assert the FTS row count matches `clipboard_display` after migration.
- Assert a delete after migration still cascades correctly.

## Documentation

- `CHANGELOG.md` entry.
- If the `index_text` split is adopted, note it in the schema doc
  comment (`schema.rs:26-32`) which currently describes
  `clipboard_display` only as "derived human-readable display text".
- ADR only if the two-column-vs-one-column question is decided in a way
  worth recording; a straightforward split does not warrant one.
