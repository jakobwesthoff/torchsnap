---
kind: feature
status: open
---

# Clipboard: Pinned entries

Allow users to pin clipboard history entries so they persist at the top
of the list regardless of age. Pinned entries are not subject to the
retention policy (30-day expiry).

## Scope

- Pin/unpin action on clipboard entries (via action palette or
  keyboard shortcut)
- Pinned entries displayed in a visually separated section at the top
  of the clipboard history list
- Pinned entries survive history cleanup
- Pin state stored in SqlStorage alongside the entry
