---
title: "Decide whether view and inline-view names may collide"
kind: investigation
component: gadget-host
impact: low
---
# Decide whether view and inline-view names may collide

The manifest keeps `views` and `inline_views` as two independent name-to-export maps, and parsing accepts the same name in both. Whether that matters depends on whether the frontend registry keys both in one namespace. If it does, the collision picks a winner silently.

## Context
`FrontendDef` (`src-tauri/src/wasm/manifest/frontend.rs`, around lines 36 to 45) carries two independent name-to-export maps, `views` and `inline_views`. Nothing rejects the same name appearing in both:

```toml
[frontend.views]
result = "FullView"

[frontend.inline-views]
result = "InlineView"
```

Manifest parsing accepts this. If view names and inline-view names share a lookup namespace anywhere (entry `data`/routing, CSS scoping, registry keys), the collision picks a winner silently.

## Impact
Unknown until the frontend registry is checked. Either the collision is harmless (separate namespaces, in which case this todo only documents that), or it is a silent component mixup.

## Options
Determine whether the launcher-side registry namespaces views and inline-views separately (`src/gadgets/wasmPluginLoader.ts` and the registry). If the namespace is shared, reject the collision at manifest parse. If it is separate, a manifest doc sentence stating that names may overlap closes this.
