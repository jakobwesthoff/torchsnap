---
kind: question
status: open
---

# Gate devtools panel behind a settings toggle

The devtools panel (tray menu entry, launcher catalog entry, window) is
currently always available in both debug and release builds. Before shipping
to end users, it should be gated behind an opt-in setting so it doesn't
clutter the UI for non-developers.

## Open questions

- Where should the toggle live? A top-level setting in the settings panel,
  a hidden preference, or a CLI flag?
- Should the toggle also control whether gadget log output is captured at
  all (saving the ring buffer memory), or only hide the UI entry points?
- Should dev-mode gadgets (GadgetSourceKind::Dev) automatically enable
  devtools?
