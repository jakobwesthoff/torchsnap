# TS `ActionId` mirror is missing the `openSettings` variant that reaches the frontend at runtime

**Kind:** possible-bug
**Severity:** low
**Area:** src/types.ts

## Problem

The frontend `ActionId` union (`src/types.ts:23-29`) mirrors the
Rust `ActionId` enum but omits one variant:

```ts
export type ActionId =
  | { type: "open" }
  | { type: "copy" }
  | { type: "reveal" }
  | { type: "openWith" }
  | { type: "delete" }
  | { type: "custom"; value: string };
```

Rust (`src-tauri/src/commands/types.rs:65-76`) additionally has
`OpenSettings`, which serializes as `{ "type": "openSettings" }`
(`#[serde(tag = "type", content = "value", rename_all =
"camelCase")]`).

This is not a theoretical variant: the zerotier gadget emits
entries whose primary action id is `ActionId::OpenSettings`
(`gadgets/zerotier/src/lib.rs:322` and `:327`, the synthetic
"configuration required" entries). Those entries cross the
bridge into `SourcedEntry.actions[].id`, so at runtime the
frontend holds `ActionId` values of a shape its own type says
cannot exist.

## Impact

No runtime misbehavior today: the launcher treats action ids as
opaque tokens — it executes `actions[actionIndex]` and passes
`action.id` straight back to `search_execute`
(`src/launcher/Launcher.tsx:551-553`, `src/lib/command.ts:46`),
never discriminating on `id.type`. Verified 2026-07-02: no
frontend file matches on `id.type`.

The risk is forward-looking but concrete: any future exhaustive
`switch` on `ActionId["type"]` (icon mapping, per-action styling
— the very use cases the Rust enum comment names at
`commands/types.rs:61-62`) will typecheck as exhaustive while
silently falling through for the openSettings entries zerotier
already produces.

## Suggested fix

Add `| { type: "openSettings" }` to the union. While there,
note the host-interception asymmetry for the next reader:
`OpenSettings` is short-circuited by `GadgetHost::execute`
(`src-tauri/src/gadget_host.rs:969-977`) and never reaches the
gadget, but the *frontend* still sees and sends it like any
other action id.
