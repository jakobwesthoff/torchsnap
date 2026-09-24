---
kind: bug
severity: medium
status: open
area: [docs/api/gadget-development.md, gadgets/gadget-sdk/wit/torchsnap-gadget.wit]
tags: [docs]
---

# `clipboard` / `frecency` permission gates missing from gadget guide and WIT docs

## Problem

The manifest parser recognizes boolean opt-ins for clipboard and
frecency (`src-tauri/src/wasm/manifest/permissions/mod.rs:72-82`):

```rust
/// `permissions.frecency` — opt-in for the WIT frecency interface ...
pub frecency: bool,
/// `permissions.clipboard` — opt-in for clipboard write.
pub clipboard: bool,
```

and the interface gate enforces them at load time: a component
that imports a gated interface without the matching permission is
rejected with "imports interfaces without matching permissions"
(`src-tauri/src/wasm/interface_gate.rs`, `is_provisioned` arms).
`docs/Gadget-Architecture/01-overview.md:254-268` documents both
gates correctly.

Two gadget-author-facing sources do not:

1. **`docs/api/gadget-development.md`** — the `[permissions]`
   schema section (`:575-605`) lists only `website-metadata`,
   `opener`, `http`, `fs`, `command`. The `clipboard` interface
   section (`:665-670`) and the `frecency` section (`:713-727`)
   describe the APIs with no mention that using them requires
   `[permissions] clipboard = true` / `frecency = true`.
2. **The WIT itself** — the `clipboard` interface doc-comment
   (`torchsnap-gadget.wit:34-49`) and the `frecency` doc-comment
   (`torchsnap-gadget.wit:115-129`) say nothing about a manifest
   gate, while the neighboring `website-metadata` comment
   explicitly documents its gate (`torchsnap-gadget.wit:856-857`)
   and `opener`/`http`/`filesystem`/`command` document theirs.
   The guide declares "when this guide and the WIT disagree, the
   WIT wins" (`gadget-development.md:11-12`), so the WIT staying
   silent on the gates matters doubly.

## Impact

A gadget author adds `clipboard::write_text` (or
`frecency::top_items`) following the documented API, ships
without the manifest flag, and the gadget fails to load with an
interface-gate error the docs never prepared them for.

## Suggested fix

Add `clipboard = true` / `frecency = true` to the
`[permissions]` example in `gadget-development.md`, one sentence
in each interface section, and gate sentences in the two WIT
doc-comments (mirroring the `website-metadata` wording). Note in
both places that enforcement happens at load time (interface
gate), not per call.
