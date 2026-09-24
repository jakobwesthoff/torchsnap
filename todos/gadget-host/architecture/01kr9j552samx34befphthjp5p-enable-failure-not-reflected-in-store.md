---
kind: bug
status: open
---

# Enable failure not reflected in settings store

## Problem

When a gadget's `enable()` fails (e.g. InterfaceGate validation rejects
it), the host sets the runtime `AtomicBool` to `false` so the gadget is
excluded from search dispatch. However the **persisted settings store**
still shows `enabled.<gadgetId> = true`.

The frontend reads enabled state from the store (via `useSetting`), not
from the runtime AtomicBool. Consequence: the settings panel shows the
gadget as "enabled" even though it's inert.

## Root cause

Two sources of truth for enabled state:

1. `GadgetSlot::enabled: Arc<AtomicBool>` — runtime dispatch gating
2. `store.get("enabled.<id>")` — persisted user preference, read by frontend

The startup flow writes `true` to the store unconditionally (line ~429
in `gadget_host.rs`). When the spawned `enable()` task fails and stores
`false` in the AtomicBool, it has no access to the Tauri store to
update the persisted value.

## Considerations

- Writing `false` to the store on enable failure would persist the
  disabled state across app restarts. The user would then need to
  manually re-enable the gadget after fixing the underlying issue
  (adding permissions to their manifest). This may or may not be
  desirable — a transient failure shouldn't permanently disable.

- A better model might be a **separate "healthy" status** surfaced to
  the frontend alongside the user's intent (`enabled`). The frontend
  would show: "enabled but failed to start" with the error reason,
  rather than silently appearing enabled or silently flipping to
  disabled.

- The `GadgetSlot::enabled` AtomicBool conflates "user wants this on"
  with "gadget is operational." Splitting into `user_enabled` (store)
  and `operational` (runtime) would let the UI distinguish between
  "user turned it off" and "it broke."

## Affected code

- `src-tauri/src/gadget_host.rs` — `initialize_and_start` Phase 2
- `src/settings/GadgetSettingsWrapper.tsx` — reads `enabled.<id>`
- `src/settings/sections/GadgetsManagementPanel.tsx` — reads `enabled.<id>`
