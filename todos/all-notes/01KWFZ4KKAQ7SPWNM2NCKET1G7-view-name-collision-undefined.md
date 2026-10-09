---
kind: question
severity: low
status: open
area: [src-tauri/src/wasm/manifest/frontend.rs]
---

# View/inline-view name collisions are not validated

## Problem
`FrontendDef` carries two independent name→export maps, `views`
and `inline_views`
(`src-tauri/src/wasm/manifest/frontend.rs:36-45`). Nothing
rejects the same name appearing in both:

```toml
[frontend.views]
result = "FullView"

[frontend.inline-views]
result = "InlineView"
```

Manifest parsing accepts this. Whether it is meaningful depends
on how the frontend registry keys components; if view names and
inline-view names share a lookup namespace anywhere (entry
`data`/routing, CSS scoping, registry keys), the collision picks
a winner silently.

## Impact
Unknown until the frontend registry is checked; either harmless
(separate namespaces, in which case this todo just documents
that) or a silent component mixup.

## Suggested fix
Determine whether the launcher-side registry namespaces views
and inline-views separately
(`src/gadgets/wasmPluginLoader.ts` / registry). If shared,
reject the collision at manifest parse; if separate, a manifest
doc sentence stating names may overlap closes this.
