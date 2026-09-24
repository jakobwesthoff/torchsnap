---
kind: decision
status: open
tags: [config]
---

# Evaluate per-gadget config files

Currently all gadget settings live in the main config engine under
`gadgets.<id>.*` keys. Evaluate whether each gadget should instead get its
own config file within its `gadget-home/<id>/` directory.

## Potential benefits

- Guaranteed namespace scoping without prefix-stripping logic.
- Cleanup on uninstall is trivial: delete the gadget-home directory and
  everything (database, config, cache) is gone. No need to scan and strip
  matching keys from the shared settings store.
- Reduces risk of key collisions or prefix-matching bugs (e.g. `calc`
  matching `calculator.*`).

## Considerations

- The settings UI currently reads from a single store. Would need to
  aggregate or proxy reads across per-gadget files.
- `on_setting_changed` notifications are driven by the shared store's
  change events. A per-gadget file would need its own file-watch or
  write-through mechanism.
- Migration path for existing installs with settings already in the shared
  store.
- Whether the frontend settings panel write path would need changes.
