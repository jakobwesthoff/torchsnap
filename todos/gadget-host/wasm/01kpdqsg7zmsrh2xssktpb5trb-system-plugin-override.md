# System Plugin Override

Allow a user-installed plugin to deliberately shadow a bundled system
plugin with the same ID. Shipped state rejects this collision outright
with an error during install; this todo tracks the follow-up work to
turn the hard rejection into an explicit, opt-in override flow.

**Status:** deferred (rejection-only in v1)

**Motivation:**
- Forking a bundled plugin while preserving its existing storage and
  settings (same ID = same SQL database, same settings keys).
- Letting a user patch or upgrade a bundled plugin independently of an
  app release, without having to rename the plugin and lose its data.

**Proposed UX:**
- Install refuses by default: "A system plugin with this ID already
  exists." (This is the v1 behaviour.)
- Settings panel exposes an "Advanced" toggle per-plugin:
  "Override system plugin". Ticking it lets the user retry the
  install (or keeps an already-installed overriding copy active).
- Overriding plugins carry a persistent badge in the plugin list, and
  a one-click "Remove override" action removes the user copy and
  restores the system plugin.

**Security and UX notes:**
- An overriding plugin inherits the system plugin's SQL storage and
  settings keys. That is the whole point of keeping the same ID — it
  is the "fork without losing state" case — but it is also the sharp
  edge that makes silent overrides unacceptable. The explicit opt-in
  is what keeps this honest.
- The WIT capability boundary is still the primary sandbox. There is
  no network interface today, so the exfiltration surface is narrow
  regardless of override status.
- When a bundled plugin version bumps in a later release, the override
  continues to shadow it. The plugin list should make that visible
  ("system: 0.3.0, user override: 0.2.0").

**Open points to revisit:**
- Should "Remove override" also offer to preserve the user copy's
  SQL database (so the user can re-apply later) or discard it?
- Should override be per-plugin only, or should a global "allow
  overrides" switch in settings be required first as an extra safety
  gate?
- Does the override flow need a confirmation dialog that spells out
  the data-inheritance implication, or is the toggle plus badge
  sufficient?
