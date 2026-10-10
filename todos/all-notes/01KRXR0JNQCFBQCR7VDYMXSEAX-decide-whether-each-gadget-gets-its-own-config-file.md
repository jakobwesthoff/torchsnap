---
title: "Decide whether each gadget gets its own config file"
kind: investigation
component: backend
status: needs-discussion
tags: [config]
---
# Decide whether each gadget gets its own config file

Gadget settings currently live in the main config engine under `gadgets.<id>.*` keys. This todo asks whether each gadget should instead get its own config file within its `gadget-home/<id>/` directory. Only the user can settle that direction.

## Potential benefits

- Guaranteed namespace scoping without prefix-stripping logic.
- Cleanup on uninstall is trivial: delete the gadget-home directory and
  everything (database, config, cache) is gone. No need to scan and strip
  matching keys from the shared settings store.
- Reduces risk of key collisions or prefix-matching bugs (e.g. `calc`
  matching `calculator.*`).

## Open questions

- The settings UI currently reads from a single store. Would need to
  aggregate or proxy reads across per-gadget files.
- `on_setting_changed` notifications are driven by the shared store's
  change events. A per-gadget file would need its own file-watch or
  write-through mechanism.
- Migration path for existing installs with settings already in the shared
  store.
- Whether the frontend settings panel write path would need changes.
