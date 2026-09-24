---
kind: feature
status: open
---

# Add settings write to WIT interface

The WIT `settings` interface currently only has `get`. WASM gadgets cannot
write settings programmatically. All writes go through the frontend settings
panel (`useGadgetSetting` → `setSetting`).

Add a `set: func(key: string, value: string)` to the WIT `settings` interface
so gadgets can update their own settings from the backend. The host already
scopes reads to `gadgets.<id>.*`; the same scoping applies to writes.

Use cases:
- A gadget that stores state in settings rather than SQL (lightweight counters,
  tokens refreshed by a background task)
- A gadget that auto-configures itself on first enable and persists the result

The settings-changed event pipeline (`CoalescingDispatcher` →
`on_setting_changed`) should fire for programmatic writes the same way it does
for frontend writes, so the frontend settings panel stays in sync.
