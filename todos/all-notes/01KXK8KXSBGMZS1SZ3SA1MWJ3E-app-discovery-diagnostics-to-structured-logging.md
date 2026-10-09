---
kind: improvement
status: open
tags: [logging]
---

# Route app discovery diagnostics into the structured logging system

All diagnostics in the app-launcher discovery path are plain
`eprintln!` to stderr:

- `src-tauri/src/platform/macos/app_discovery.rs:127` —
  `"skipping {}: {e:#}"` (per-bundle Info.plist parse failure)
- `src-tauri/src/gadgets/app_launcher.rs:188` —
  `"initial app discovery failed: {e:#}"`
- `src-tauri/src/gadgets/app_launcher.rs:118` —
  `"background app discovery failed: {e:#}"`
- `src-tauri/src/icons/icon_cache.rs:94,105,112` — icon extraction /
  cache write failures

A release build launched via Finder/LaunchServices has no captured
stderr, so these messages are discarded. The host already has a
structured logging pipeline (`LoggingSystem` in
`src-tauri/src/wasm/logging/`, in-memory ring buffer surfaced in the
DevTools console), but the discovery path does not feed it. During the
2026-07-15 missing-apps investigation
(`01kxk8kxsaed0ah2q0z6eamhg2-app-discovery-fallback-directory-scan.md`)
there was consequently no log trail to inspect.

## Intent

Emit discovery lifecycle events and failures as `LogItem`s through the
existing `log_sender` channel instead of (or in addition to) stderr:
scan started, scan finished with result count, per-bundle skips, and
scan failures. That makes discovery behaviour observable in the
DevTools console of a production build.

## Related

- `todos/gadget-host/wasm/01kqmdf6rgavhar6m4mm8vhxeb-redirect-gadget-stdout-to-logger.md`
  addresses the same stderr-bypass problem for WASM gadget stdio.
