---
title: "Add an explicit opt-in override for bundled system gadgets"
kind: feature
component: gadget-host
status: needs-discussion
horizon: someday
tags: [security]
---
# Add an explicit opt-in override for bundled system gadgets

A user-installed gadget may not yet deliberately shadow a bundled system gadget with the same ID. Installing such a gadget is rejected outright with an error. This todo tracks the follow-up work that turns the hard rejection into an explicit, opt-in override flow.

**Motivation:**
- Forking a bundled gadget while preserving its existing storage and settings (same ID means the same SQL database and the same settings keys).
- Letting a user patch or upgrade a bundled gadget independently of an app release, without renaming the gadget and losing its data.

**Proposed UX:**
- Install refuses by default with "A system gadget with this ID already exists." (This is the v1 behaviour.)
- The settings panel exposes an "Advanced" toggle per gadget: "Override system gadget". Ticking it lets the user retry the install, or keeps an already-installed overriding copy active.
- Overriding gadgets carry a persistent badge in the gadget list. A one-click "Remove override" action removes the user copy and restores the system gadget.

**Security and UX notes:**
- An overriding gadget inherits the system gadget's SQL storage and settings keys. That is the point of keeping the same ID (the "fork without losing state" case), but it is also the sharp edge that makes silent overrides unacceptable. The explicit opt-in is what keeps this honest.
- The WIT capability boundary is still the primary sandbox. There is no network interface today, so the exfiltration surface is narrow regardless of override status.
- When a bundled gadget version bumps in a later release, the override continues to shadow it. The gadget list should make that visible ("system: 0.3.0, user override: 0.2.0").

## Open questions

- Should "Remove override" also offer to preserve the user copy's SQL database (so the user can re-apply it later), or discard it?
- Should the override be per gadget only, or should a global "allow overrides" switch in settings be required first as an extra safety gate?
- Does the override flow need a confirmation dialog that spells out the data-inheritance implication, or are the toggle and badge sufficient?

## Revisit when

No trigger known; reconsider at the next sweep.
