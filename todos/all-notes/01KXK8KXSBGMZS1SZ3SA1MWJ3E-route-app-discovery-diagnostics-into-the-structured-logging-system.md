---
title: "Route app discovery diagnostics into the structured logging system"
kind: improvement
component: backend
tags: [logging]
---
# Route app discovery diagnostics into the structured logging system

App discovery reports its diagnostics with bare `eprintln!` to stderr. A release build launched through Finder or LaunchServices has no captured stderr, so these messages are discarded. The host already has a structured logging pipeline (`LoggingSystem` in `src-tauri/src/wasm/logging/`, an in-memory ring buffer surfaced in the DevTools console), but the discovery path does not feed it. During the 2026-07-15 missing-apps investigation there was consequently no log trail to inspect.

## Affected call sites

- `src-tauri/src/platform/macos/app_discovery.rs` (around line 127): `"skipping {}: {e:#}"`, a per-bundle Info.plist parse failure.
- `src-tauri/src/gadgets/app_launcher.rs` (around line 188): `"initial app discovery failed: {e:#}"`.
- `src-tauri/src/gadgets/app_launcher.rs` (around line 118): `"background app discovery failed: {e:#}"`.
- `src-tauri/src/icons/icon_cache.rs` (around lines 94, 105, 112): icon extraction and cache write failures.

## Intent

Emit discovery lifecycle events and failures as `LogItem`s through the existing `log_sender` channel, instead of or in addition to stderr: scan started, scan finished with result count, per-bundle skips, and scan failures. That makes discovery behavior observable in the DevTools console of a production build.

## Relations

- Relates to: [Add a directory-scan fallback when Spotlight results are implausible](01KXK8KXSAED0AH2Q0Z6EAMHG2-add-a-directory-scan-fallback-when-spotlight-results-are-implausible.md), the discovery work behind the 2026-07-15 missing-apps investigation
- Relates to: [Route gadget stdout and stderr to the host logging pipeline](01KQMDF6RGAVHAR6M4MM8VHXEB-route-gadget-stdout-and-stderr-to-the-host-logging-pipeline.md), which addresses the same stderr-bypass problem for WASM gadget stdio
