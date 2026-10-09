---
kind: feature
status: open
---

# Frecency stats: top-N most used items with display text

Show the top 5 most frequently used items per gadget in the frecency
settings stats view, with human-readable titles instead of raw IDs.

## Approach

Add a `frecency_items` lookup table:

```sql
CREATE TABLE frecency_items (
    gadget_id    TEXT NOT NULL,
    item_id      TEXT NOT NULL,
    display_text TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (gadget_id, item_id)
) WITHOUT ROWID;
```

Upsert on every `record()` call so display text stays current. The
`record()` signature gains a `display_text: &str` parameter.

For host-level recording in `GadgetHost::execute()`, the display text
needs to come from somewhere — the frontend already has `ScoredEntry.title`
when calling `search_execute`, so adding a `display_text` param to that
Tauri command is the path of least resistance.

For direct gadget `record()` calls, the gadget provides the text itself.

Join against `frecency_items` in the stats query to show top-N items
with readable names.

## Depends on

- Frecency system (core implementation)
