---
title: "Document the clipboard and frecency manifest gates for gadget authors"
kind: docs
component: gadget-sdk
---
# Document the clipboard and frecency manifest gates for gadget authors

A gadget that uses the clipboard or frecency interfaces needs
`clipboard = true` or `frecency = true` under `[permissions]` in its
manifest, or it fails to load. The gadget guide and the WIT doc-comments do
not say so, although the manifest parser and the interface gate enforce it.

## Problem

Read in the code and docs.

The manifest parser recognizes boolean opt-ins for clipboard and
frecency (the `frecency` and `clipboard` fields of the permissions struct in `src-tauri/src/wasm/manifest/permissions/mod.rs`):

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
`docs/Gadget-Architecture/01-overview.md` (the capability table) documents both
gates correctly.

Two gadget-author-facing sources do not:

1. **`docs/api/gadget-development.md`**: the `[permissions]`
   schema section lists only `website-metadata`,
   `opener`, `http`, `filesystem`, `command`. The `clipboard` interface
   section and the `frecency` section describe the APIs with no mention that using them requires
   `[permissions] clipboard = true` / `frecency = true`.
2. **The WIT itself** (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit`): the
   `clipboard` interface doc-comment and the `frecency`
   doc-comment say nothing about a manifest
   gate, while the neighboring `website-metadata` comment
   explicitly documents its gate
   and `opener`/`http`/`filesystem`/`command` document theirs.
   The guide declares "when this guide and the WIT disagree, the
   WIT wins" (the opening of `gadget-development.md`), so the WIT staying
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
