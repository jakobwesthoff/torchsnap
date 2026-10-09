---
kind: improvement
severity: medium
status: open
area: [src-tauri/src]
---

# Host-side errors go only to stderr (`eprintln!`) — invisible in a packaged app

The pattern is codebase-wide: `rg -c "eprintln!" src-tauri/src` counts
67 call sites across 19 files (up from ~30 when this todo was
written).

## Problem

All host-side error reporting outside the WASM runtime uses bare
`eprintln!`. There is no `tracing`/`log` crate in
`src-tauri/Cargo.toml` at all; the only structured logging
infrastructure is the gadget-log pipeline under
`src-tauri/src/wasm/logging/`, which feeds devtools but is not
used by host code.

Representative call sites:

- `src-tauri/src/lib.rs` (19 sites): window creation, chrome
  hiding, launcher frame/show/hide/warm-up failures, event-emit
  failures, gadget-uninstall cleanup.
- `src-tauri/src/gadget_host.rs` (10 sites): shortcut registration
  failures, layout races, emit failures.
- `src-tauri/src/platform/macos/app_discovery.rs:127`,
  `.../settings_discovery.rs:65`,
  `.../launcher_panel.rs:142`: discovery parse skips, panel
  retrieval failures inside main-thread closures.
- `src-tauri/src/network/website_metadata/favicon_store.rs:187`,
  `.../fetch.rs:197`, `src-tauri/src/icons/icon_cache.rs:94,105,112`,
  `src-tauri/src/gadgets/system_preferences.rs:119`.

When Torchsnap runs as a bundled `.app` (not launched from a
terminal), stderr is not attached to anything the user or
developer can see. Every one of these failure reports — including
operationally significant ones like "failed to show launcher",
"shortcut: failed to register shortcuts", and "failed to create
window" — is silently discarded.

## Impact

Field debugging is nearly impossible: the launcher can fail to
show, shortcuts can silently stop registering, icon extraction can
fail wholesale, and the packaged app leaves no trace. The devtools
log view shows gadget logs but none of these host events, which is
misleading when triaging ("no errors in devtools" does not mean no
errors).

## Suggested fix

Decide on one host-side logging path and route the existing call
sites through it. Options, not mutually exclusive:

1. Feed host errors into the existing gadget-log storage under a
   reserved source (e.g. `host`) so they appear in devtools next
   to gadget logs.
2. Add `tracing` + a file appender under the app data dir for the
   packaged build.

Either way, a small `host_log!`-style helper (or `tracing` macros)
replacing the bare `eprintln!` calls keeps the change mechanical.
