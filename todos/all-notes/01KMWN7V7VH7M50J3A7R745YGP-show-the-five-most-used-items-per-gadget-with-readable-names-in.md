---
title: "Show the five most used items per gadget with readable names in frecency stats"
kind: feature
component: backend
---

# Show the five most used items per gadget with readable names in frecency stats

The frecency settings stats view shows raw item IDs. This todo shows the
top five most frequently used items per gadget, with human-readable
titles instead of IDs.

## Proposal

Add a `frecency_items` lookup table:

```sql
CREATE TABLE frecency_items (
    gadget_id    TEXT NOT NULL,
    item_id      TEXT NOT NULL,
    display_text TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (gadget_id, item_id)
) WITHOUT ROWID;
```

Upsert on every `record()` call so the display text stays current. The
`record()` signature gains a `display_text: &str` parameter.

For host-level recording in `GadgetHost::execute()`, the display text
has to come from somewhere. The frontend already has `ScoredEntry.title`
when it calls `search_execute`, so adding a `display_text` parameter to
that Tauri command is the path of least resistance.

For direct gadget `record()` calls, the gadget provides the text itself.

Join against `frecency_items` in the stats query to show the top-N items
with readable names.
