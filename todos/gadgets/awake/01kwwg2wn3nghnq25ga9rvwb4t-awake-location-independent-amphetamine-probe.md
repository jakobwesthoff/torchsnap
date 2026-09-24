---
kind: improvement
status: open
---

# Awake: location-independent Amphetamine availability probe

The awake gadget's Amphetamine availability probe checks the fixed
path `/Applications/Amphetamine.app/Contents/Info.plist`, granted via
the gadget's filesystem permission (`gadgets/awake/manifest.toml`) and
read in `select_backend` (`gadgets/awake/src/backend/mod.rs`, which
already carries a `TODO` noting this). An Amphetamine install outside
`/Applications` — `~/Applications` or a custom location — is not
detected, so the backend never activates for it and the gadget stays
silent.

The AppleScript control channel the backend drives
(`gadgets/awake/src/backend/amphetamine.rs`) addresses Amphetamine by
name (`tell application "Amphetamine" ...`) and is location-independent;
only the availability probe is path-bound.

## Why deferred

Host-side, a bundle-identifier-to-path resolution already exists
(`src-tauri/src/platform/macos/app_resolver.rs`, via LaunchServices),
but it is not exposed to gadgets as a queryable capability. The
`app-icon` entry-icon variant added for real Amphetamine icons
(ADR 0045) deliberately returns no resolution outcome to the guest —
a gadget cannot observe whether an application is installed through
that variant, by design.

Decided 2026-07-06: fixing the probe is out of scope for the app-icon
work. Any fix needs its own design for a host-side app-presence
surface usable from WASM gadgets, rather than reusing `app-icon`'s
resolution path.

## References

- `gadgets/awake/manifest.toml` — filesystem permission scoped to the
  fixed probe path
- `gadgets/awake/src/backend/mod.rs` — `select_backend`, existing
  `TODO` on the probe
- `gadgets/awake/src/backend/amphetamine.rs` — AppleScript control
  channel, location-independent
- `src-tauri/src/platform/macos/app_resolver.rs` — existing
  bundle-id → path resolution, not exposed to gadgets
- `docs/adr/0045-app-icon-entry-icon-variant.md` — `app-icon` entry
  icon variant; resolution outcome is not observable by the guest
