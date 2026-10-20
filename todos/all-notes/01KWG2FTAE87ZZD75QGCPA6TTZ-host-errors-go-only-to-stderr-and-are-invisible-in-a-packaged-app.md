---
title: "Host errors go only to stderr and are invisible in a packaged app"
kind: improvement
component: backend
status: needs-discussion
tags: [logging]
---
# Host errors go only to stderr and are invisible in a packaged app

Host-side error reporting outside the WASM runtime uses bare `eprintln!`. When Torchsnap runs as a bundled `.app`, stderr is attached to nothing, so these reports are discarded. Operationally significant failures such as "failed to show launcher", "shortcut: failed to register shortcuts" and "failed to create window" leave no trace, so field debugging is nearly impossible. The pattern is codebase-wide: `rg -c "eprintln!" src-tauri/src` counts 74 call sites across 21 files (checked 2026-10-21), up from about 30 when this todo was written.

## Problem

All host-side error reporting outside the WASM runtime uses bare `eprintln!`. There is no `tracing` or `log` crate in `src-tauri/Cargo.toml`. The only structured logging infrastructure is the gadget-log pipeline under `src-tauri/src/wasm/logging/`, which feeds devtools but is not used by host code.

Representative call sites:

- `src-tauri/src/lib.rs` (19 sites): window creation, chrome hiding, launcher frame/show/hide/warm-up failures, event-emit failures, gadget-uninstall cleanup.
- `src-tauri/src/gadget_host.rs` (10 sites): shortcut registration failures, layout races, emit failures.
- `src-tauri/src/platform/macos/app_discovery.rs` (`apps_from_bundle_paths`), `src-tauri/src/platform/macos/settings_discovery.rs` (around line 65), `src-tauri/src/platform/macos/launcher_panel.rs` (around line 142): discovery parse skips, panel retrieval failures inside main-thread closures.
- `src-tauri/src/network/website_metadata/favicon_store.rs` (around line 187), `src-tauri/src/network/website_metadata/fetch.rs` (around line 197), `src-tauri/src/icons/icon_cache.rs` (around lines 94, 105, 112), `src-tauri/src/gadgets/system_preferences.rs` (around line 119).

The devtools log view shows gadget logs but none of these host events. That is misleading when triaging: "no errors in devtools" does not mean no errors.

A case from the field (2026-10-05): an in-app update download failed on a DNS lookup of github.com. `src-tauri/src/updates/mod.rs` logs the plugin's error with `eprintln!` only, and the bundled app's stderr is `/dev/null` (checked with `lsof` on the running process). The cause had to be pieced together from mDNSResponder entries in the macOS unified log.

## Impact

Field debugging is nearly impossible: the launcher can fail to show, shortcuts can silently stop registering, icon extraction can fail wholesale, and the packaged app leaves no trace.

## Suggested fix

Decide on one host-side logging path and route the existing call sites through it. The options are not mutually exclusive:

1. Feed host errors into the existing gadget-log storage under a reserved source (for example `host`) so they appear in devtools next to gadget logs.
2. Add `tracing` plus a file appender under the app data dir for the packaged build.

Either way, a small `host_log!`-style helper (or `tracing` macros) replacing the bare `eprintln!` calls keeps the change mechanical.
