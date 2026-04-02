# Bangs plugin: `!` prefix mode with custom UI

## Overview

Register `!` as a search prefix for the bangs plugin, enabling an exclusive
browsing/search mode with a dedicated custom UI — similar to how `:` activates
the emoji picker.

## Behaviour

- Typing `!` alone activates the bang browser, showing a custom UI with:
  - Recently/frequently used bangs (frecency-ranked) at the top
  - A searchable list of all available bangs
  - Category grouping or filtering (data available in the `category` column)
  - Service name, trigger, and domain displayed per entry
- Typing `!` followed by text fuzzy-searches through bang triggers and service
  names (e.g. `!goo` narrows to Google, Google Maps, etc.)
- Selecting a bang from the list inserts it into the query or immediately
  prompts for search terms

## Relationship to current implementation

The current bangs plugin participates in general search (no prefix) and detects
exact `!<trigger>` tokens. The prefix mode would coexist:

- `!` alone or `!` + partial text → prefix mode, custom UI, bang browsing
- Exact `!g foo bar` in a mixed query → current general search path, single
  result entry

This requires the plugin to register `!` via `search_prefixes()` while still
participating in general search. The hybrid routing approach (prefix + general)
from the original plan discussion.

## Prerequisites

- Frecency tracking is already in place (entry_id is `!<trigger>`)
- Category data is stored in the `bangs` table
- Need a frontend custom UI component (like `EmojiGrid` for the emoji picker)

## Notes

- Consider whether the custom UI should show favicons once the favicon
  fetching service is available (`01kn829tr1dp9jfnjta36xkzqy`)
- The fuzzy search within prefix mode could use nucleo matching on trigger +
  service_name, similar to how the emoji picker matches on shortcodes + labels
