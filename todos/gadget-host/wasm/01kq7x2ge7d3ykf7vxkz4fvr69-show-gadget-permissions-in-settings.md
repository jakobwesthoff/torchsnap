# Surface gadget permissions in the settings UI

## Context

`GadgetsManagementPanel.tsx` currently shows each gadget's name,
description, source badge, and an enable toggle. It does **not** display
the capabilities each gadget has been granted via its manifest
(`[permissions.opener]`, `[permissions.http]`, future
`[[permissions.process]]`).

For URL/HTTP this gap was acceptable while the surface was small. With
process execution about to land the gap becomes more
significant: a gadget that can spawn `mdfind` or `git` is a categorically
larger trust ask than one that can only open `https` URLs, and users
deserve to see what they have running.

## Target

Extend the settings panel to render an inline permissions block per
gadget card, fed from the parsed manifest:

- `[permissions.opener]` — list allowed schemes and (once added) path
  roots.
- `[permissions.http]` — list allowed origins; render `*` as a prominent
  warning chip.
- `[[permissions.process]]` — list each rule's binary plus a
  human-readable summary of its argv constraints. `*` argv (free-form)
  should render as a warning chip the same way `*` HTTP origins do.

Permissions that are absent should not render an empty section; the
shape of the block should reflect the gadget's actual surface.

## Non-goals

- Editing or revoking permissions. Permissions remain manifest-declared
  and immutable per install. Hot-toggling them is a separate, much
  larger design question (would invalidate cached `WasmGadgetInstance`
  state).
- Live capability metering ("gadget X has made N HTTP calls in the last
  hour"). Logging of capability calls is an existing concern, separate
  from this UI surface.

## Blocked-by / enables

- Depends on: `[[permissions.process]]` manifest + WIT landing first so
  there is a real surface to render.
- Pairs with the install-time consent prompt todo (sibling file in
  `todos/gadget-host/wasm/`).
