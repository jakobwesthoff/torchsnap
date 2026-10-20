---
title: "Add a directory-scan fallback when Spotlight results are implausible"
kind: bug
component: platform
origin: incident
---

# Add a directory-scan fallback when Spotlight results are implausible

`MdfindDiscovery::discover()` relies exclusively on
`mdfind "kMDItemContentType == 'com.apple.application-bundle'"`
(`MdfindDiscovery::discover` in `src-tauri/src/platform/macos/app_discovery.rs`). There is no
filesystem fallback. If Spotlight's metadata store is missing entries,
`mdfind` still exits 0, and `discover()` returns a successful but
incomplete result.

## Incident (2026-07-15)

On the development machine, the exact query the app runs returned 290
bundles, of which only 65 survived the `/Applications/` path filter:
64 from `/System/Applications/` and a single one from `/Applications/`
(Safari.app). `/Applications` contained 61 apps at the time, including
Ghostty.app (installed 2026-03-15). `mdls /Applications/Ghostty.app`
returned no metadata at all, while `mdutil -s /` reported
"Indexing enabled." The Data-volume Spotlight store was missing
user-installed apps without any error surfacing. Every 5-minute background
refresh (`src-tauri/src/gadgets/app_launcher.rs:82-123`) re-ran the same
query, got the same incomplete answer, and replaced the cache, so the
launcher stayed broken indefinitely.

## Intent

Add a guard against silently incomplete Spotlight results:

- Plausibility check: compare the discovery result against a plain
  `read_dir` of `/Applications` (one directory listing, which is cheap).
  If bundles present on disk are absent from the `mdfind` result, merge
  them in via the existing `try_discover_app` Info.plist path and log a
  warning.
- Alternatively, drop `mdfind` for the standard directories entirely.
  Scan `/Applications`, `~/Applications` and `/System/Applications`
  directly, and keep `mdfind` only for apps in non-standard locations.

The evaluation of replacing `mdfind` with a direct API already lists
directory scanning as one option. This incident is a correctness argument
for that switch, not just a performance one.

## Relations

- Relates to: [Decide whether macOS app discovery should use a direct API instead of mdfind](01KMJPBR2SAEHJHHGKH3JDSGNA-decide-whether-macos-app-discovery-should-use-a-direct-api-instead-of.md), option 4 there is directory scanning, and this incident adds a correctness argument for it.
