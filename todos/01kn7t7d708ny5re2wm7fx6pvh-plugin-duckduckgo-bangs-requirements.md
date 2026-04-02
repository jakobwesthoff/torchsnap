# Plugin: DuckDuckGo Bangs — Requirements

Supersedes `01kmxhptrqdnhjqajz2ymxnw7p-plugin-duckduckgo-bangs.md` with
concrete design decisions and implementation requirements.

## Overview

A standalone plugin that detects DuckDuckGo bang patterns (e.g. `!crates`,
`!g`, `!yt`) anywhere in the query string and surfaces a single result entry
that opens the corresponding service URL with the remaining query terms.

---

## 1. Bang Data Source

### 1.1 Baked-in Fallback

- A `just` task downloads the DuckDuckGo `bang.json` file and stores it in
  `derived/bang.json` (checked into version control).
- This file serves as a compile-time fallback baked into the plugin binary via
  `include_str!` or `include_bytes!`.

### 1.2 Just Task

- New recipe in `just/assets.just` (or a new `just/bangs.just` — TBD):
  ```
  asset-bang-data:
      curl -sL 'https://duckduckgo.com/bang.js' -o derived/bang.json
  ```
- The `assets` meta-recipe should include this new task.
- The file is committed to `derived/` so builds work without network access.

---

## 2. Plugin Initialization (Background)

Initialization runs in a background thread (inside `setup()`). While
initialization is in progress, `search()` returns no results.

### 2.1 Startup Flow

```
1. Check plugin-local SqlStorage for an existing, populated bang table.
   → If present and valid: use it. Done.

2. Attempt to download a fresh bang.json via network::Http.
   → On success: parse and import into SqlStorage. Done.

3. On any download failure (network error, timeout, bad response):
   → Fall back to the baked-in derived/bang.json.
   → Parse and import into SqlStorage.
```

### 2.2 SqlStorage Schema

- Database: `<app_data_dir>/plugins/duckduckgo_bangs/bangs.db`
- Store parsed bang entries in a table optimized for prefix/exact-match lookup
  by bang trigger (the `!xxx` keyword).
- Index on the trigger column for fast lookup.
- Store metadata (import timestamp, source — "network" or "builtin", entry
  count, distinct domain count) in a separate metadata table for the settings
  UI.

### 2.3 Readiness Signal

- An `AtomicBool` (or similar) flag that `search()` checks. While `false`,
  `search()` returns immediately with no results.
- Set to `true` once SqlStorage is populated and ready.

---

## 3. Query Detection & Search

### 3.1 Bang Pattern Detection

- Scan the query string for a token matching `!<identifier>` anywhere in the
  input (start, middle, end).
- Match the identifier against the SqlStorage bang table (exact match on
  trigger).
- If no valid bang is found, return no results (let other plugins handle the
  query).

### 3.2 Result Entry

- Always return exactly **one** entry when a valid bang is detected.
- Title: `Open '<search terms>' in <service name>`
  - `<service name>` comes from the bang definition (e.g. "Crates.io",
    "Google", "YouTube").
  - `<search terms>` is the query with the bang token removed (see §4).
- Icon: a Heroicon (specific icon TBD — e.g. `ArrowTopRightOnSquare` or
  `GlobeAlt`). Favicon fetching is a future enhancement, not part of the
  initial implementation.

---

## 4. Bang Token Removal & Query Construction

When building the search terms from the raw query, the bang token and its
surrounding whitespace must be removed intelligently:

| Raw query         | Bang    | Search terms |
|-------------------|---------|--------------|
| `!g foo bar`      | `!g`    | `foo bar`    |
| `foo !baz bar`    | `!baz`  | `foo bar`    |
| `xxx yyy !zzz`    | `!zzz`  | `xxx yyy`    |
| `!g`              | `!g`    | *(empty)*    |

Rules:
1. Remove the bang token itself.
2. Collapse any resulting double-space into a single space.
3. Trim leading/trailing whitespace.

---

## 5. Execution

When the user activates the result entry:

1. Take the bang's URL template (contains `{{{s}}}` placeholder).
2. URL-encode the cleaned search terms (§4).
3. Substitute the encoded terms into the URL template.
4. Open the resulting URL via Tauri's shell open API.

---

## 6. Settings UI

### 6.1 Enable/Disable Toggle

- Standard plugin enable/disable switch.
- Setting key: `plugins.duckduckgo_bangs.enabled` (default: `true`).

### 6.2 Bang Database Stats

Display read-only stats fetched via `handle_message("stats", {})`:

- **Import date** — when the currently loaded bang data was imported.
- **Data source** — whether the data came from a network fetch or the
  baked-in fallback.
- **Total bangs** — number of bang entries.
- **Unique domains/services** — distinct target domains.

### 6.3 Refresh Button

- A button labeled "Refresh Bang Database" (or similar).
- Triggers a `handle_message("refresh", {})` call.
- Backend re-downloads `bang.json` via `network::Http`, re-parses, and
  re-imports into SqlStorage.
- On failure, show an error in the UI (keep existing data intact).
- On success, update the displayed stats.

---

## 7. Implementation Checklist

Rough ordering — dependencies flow top-to-bottom:

- [ ] Add `just` task to download `bang.json` into `derived/`
- [ ] Run the task, commit `derived/bang.json`
- [ ] Create `src-tauri/src/plugins/duckduckgo_bangs/` module directory
- [ ] Define SqlStorage schema and migrations
- [ ] Implement bang.json parser (handle the DDG JSON format)
- [ ] Implement background initialization flow (§2.1)
- [ ] Implement `search()` with bang detection and result construction (§3)
- [ ] Implement bang token removal logic (§4)
- [ ] Implement `execute()` with URL construction and Tauri open (§5)
- [ ] Register plugin in `plugins/mod.rs` and `plugin_host.rs`
- [ ] Implement `initialize_settings()` with enabled default
- [ ] Implement `handle_message` for `stats` and `refresh`
- [ ] Create settings UI component with stats display and refresh button
- [ ] Register settings component in the settings router/navigation

---

## Open Questions / Future Work

- **Favicon fetching**: fetch and cache service favicons for richer result
  display. Deferred to a follow-up.
- **Frecency integration**: when `!` is typed alone, show recently/frequently
  used bangs. Deferred.
- **Fuzzy matching**: suggest close bang matches on typos. Deferred.
- **Multiple bang tokens**: what if the query contains more than one bang?
  Current design: match the first one found, ignore the rest.
