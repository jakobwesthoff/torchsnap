---
kind: feature
status: open
---

# Bangs gadget: `!` prefix mode with custom UI

## Overview

Register `!` as a search prefix for the bangs gadget, enabling an exclusive
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

The current bangs gadget participates in general search (no prefix) and detects
exact `!<trigger>` tokens. The prefix mode would coexist:

- `!` alone or `!` + partial text → prefix mode, custom UI, bang browsing
- Exact `!g foo bar` in a mixed query → current general search path, single
  result entry

This requires the gadget to register `!` via `search_prefixes()` while still
participating in general search. The hybrid routing approach (prefix + general)
from the original plan discussion.

## Prerequisites

- Frecency tracking is already in place (entry_id is `!<trigger>`)
- Category data is stored in the `bangs` table
- Need a frontend custom UI component (like `EmojiGrid` for the emoji picker)

## Notes

- The custom UI should render favicons per entry via the SDK helper
  (`torchsnap_gadget_sdk::website_metadata::favicon_or`); the host's
  metadata service and the wrapper are in place, the bangs gadget's
  general-search path already uses them.
- The fuzzy search within prefix mode could use nucleo matching on trigger +
  service_name, similar to how the emoji picker matches on shortcodes + labels
