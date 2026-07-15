# App discovery: fallback directory scan when Spotlight results are implausible

`MdfindDiscovery::discover()` relies exclusively on
`mdfind "kMDItemContentType == 'com.apple.application-bundle'"`
(`src-tauri/src/platform/macos/app_discovery.rs:107`). There is no
filesystem fallback: if Spotlight's metadata store is missing entries,
`mdfind` still exits 0 and `discover()` returns a successful but
incomplete result.

## Incident (2026-07-15)

On the development machine, the exact query the app runs returned 290
bundles, of which only 65 survived the `/Applications/` path filter:
64 from `/System/Applications/` and a single one from `/Applications/`
(Safari.app). `/Applications` contained 61 apps at the time, including
Ghostty.app (installed 2026-03-15). `mdls /Applications/Ghostty.app`
returned no metadata at all while `mdutil -s /` reported
"Indexing enabled." — the Data-volume Spotlight store was missing
user-installed apps without any error surfacing. Every 5-minute
background refresh (`src-tauri/src/gadgets/app_launcher.rs:82-123`)
re-ran the same query, got the same incomplete answer, and replaced the
cache, so the launcher stayed broken indefinitely.

## Intent

Add a guard against silently incomplete Spotlight results:

- Plausibility check: compare the discovery result against a plain
  `read_dir` of `/Applications` (cheap, one directory listing). If
  bundles present on disk are absent from the `mdfind` result, merge
  them in via the existing `try_discover_app` Info.plist path and log
  a warning.
- Alternatively, drop `mdfind` for the standard directories entirely
  and scan `/Applications`, `~/Applications`, `/System/Applications`
  directly, keeping `mdfind` only for apps in non-standard locations.

## Related

- `todos/backend/search/01kmjpbr2saehjhhgkh3jdsgna-app-discovery-direct-api.md`
  already evaluates replacing `mdfind` (option 4 is directory
  scanning); this incident is a correctness argument for that switch,
  not just a performance one.
- `todos/backend/search/01kxk8kxsbgmzs1sz3sa1mwj3f-app-discovery-validate-cache-removals.md`
  covers protecting the existing cache when a refresh shrinks.
