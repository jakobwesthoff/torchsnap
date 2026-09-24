---
kind: feature
status: open
---

# Add arbitrary data field to ScoredEntry for custom UIs

## Problem

`ScoredEntry` has a fixed set of fields (`id`, `title`, `subtitle`, `icon`,
`score`, etc.). Gadgets that render their own custom UI via `CustomUI` or
`InlineUI` sometimes need to pass per-entry metadata that doesn't map to any
existing field (e.g., timestamps, result types, raw values).

Currently the only workaround is encoding extra data into existing fields
(stuffing timestamps into IDs or subtitles) which is fragile and mixes
display concerns with data transport.

## Proposed solution

Add an optional opaque data field to `ScoredEntry`:

```rust
pub struct ScoredEntry {
    // ... existing fields ...
    /// Gadget-specific metadata passed through to the frontend.
    /// Only meaningful when the gadget renders its own UI component.
    pub data: Option<serde_json::Value>,
}
```

This would serialize through `SourcedEntry` to the frontend, where custom UI
components can read it from each entry without parsing conventions out of
display fields.

## Concrete use case

The calculator gadget's history entries need a `computed_at` timestamp for
"x ago" display in the history list. Currently there's no clean way to pass
this per-entry without abusing the subtitle or ID fields.
