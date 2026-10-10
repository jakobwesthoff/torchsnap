---
title: "Add a settings write to the WIT settings interface"
kind: feature
component: gadget-host
tags: [api-design]
---
# Add a settings write to the WIT settings interface

The WIT `settings` interface only has `get`, so WASM gadgets cannot write settings programmatically. All writes go through the frontend settings panel (`useGadgetSetting` to `setSetting`). This todo adds `set: func(key: string, value: string)` to the `settings` interface, so gadgets can update their own settings from the backend.

The host already scopes reads to `gadgets.<id>.*`, and the same scoping applies to writes.

Use cases:
- A gadget that stores state in settings rather than SQL (lightweight counters, tokens refreshed by a background task).
- A gadget that configures itself on first enable and persists the result.

The settings-changed event pipeline (`CoalescingDispatcher` to `on_setting_changed`) should fire for programmatic writes the same way it does for frontend writes, so the frontend settings panel stays in sync.
