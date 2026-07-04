# Clipboard search feeds raw user input to FTS5 MATCH — operators and quotes break the query

**Kind:** bug
**Severity:** medium
**Area:** src-tauri/src/gadgets/clipboard/storage.rs

## Problem

`search_history` builds the FTS5 match expression by appending `*`
to the raw user query
(`src-tauri/src/gadgets/clipboard/storage.rs:76-89`):

```rust
Some(term) if !term.is_empty() => {
    let fts_query = format!("{term}*");
    self.sql.query_map(
        "... WHERE clipboard_fts MATCH ?1 ORDER BY rank",
        &[SqlValue::from(fts_query)],
        ...
```

FTS5 MATCH input is a query language, not a plain string. Typing
any of the following into the clipboard history search box makes
SQLite return a query-syntax error, which propagates out of
`search_history`:

- a double quote `"` (unterminated string)
- `(` or `)` (grouping)
- `AND`/`OR`/`NOT`/`NEAR` in the "wrong" position
- a leading `-` or `^`, a bare `*`, a `:` (column filter syntax)

Copied text is arbitrary, so searching for it with punctuation is
the normal case, not an edge case (e.g. searching for a code
snippet `foo(`, or a quoted phrase).

Two callers are affected differently:

- `handle_message("search", ...)` (`mod.rs:417-445`) returns the
  error to the frontend — search UI shows a failure (or nothing,
  depending on frontend handling) instead of results.
- `refresh_active_query` (`storage.rs:465-475`) logs to stderr and
  returns — the stored query keeps failing on every data change,
  so the open UI silently stops receiving updates while that
  query is active.

Additionally, `format!("{term}*")` appends `*` to the *whole*
string: for multi-word input only the last token is
prefix-matched, and if the input ends in punctuation the trailing
`*` is itself a syntax error.

## Impact

Searching clipboard history for text containing FTS5
metacharacters (quotes, parentheses, operators) errors out instead
of returning matches. No data corruption; pure functional
breakage of search.

## Suggested fix

Sanitize into an explicit token query: split the user input on
whitespace, strip/escape embedded double quotes (`"` → `""`),
wrap every token in double quotes, and append `*` per token:

```text
input:  he said "foo(
query:  "he" "said" "foo("*
```

This turns all input into literal token matching with per-token
prefix search and removes the operator surface entirely. Add
tests with quotes, parens, `AND`, trailing punctuation, and
non-ASCII input.
