# App discovery claims background-app filtering but performs none; nested helper bundles pass the directory check

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/platform/macos/app_discovery.rs

## Problem

Three related inconsistencies in macOS app discovery:

1. **No visibility filtering despite claims.** The `AppDiscovery`
   trait contract says "Background agents, UI-less helpers, and
   other non-launchable bundles should be filtered out by the
   implementation" (`src-tauri/src/platform/app_discovery.rs:61-63`),
   and the macOS module header says it parses Info.plist "for
   display name and visibility metadata"
   (`src-tauri/src/platform/macos/app_discovery.rs:9-11`). The
   implementation reads only `CFBundleDisplayName`, `CFBundleName`,
   and `CFBundleIdentifier` (`try_discover_app`,
   `app_discovery.rs:67-100`). It never checks `LSUIElement` or
   `LSBackgroundOnly`, so UI-less/background bundles that pass the
   directory check are listed as launchable apps.

2. **Vestigial `Option` return.** `try_discover_app` returns
   `anyhow::Result<Option<DiscoveredApp>>` but has no code path
   returning `Ok(None)`. The caller's match arm
   (`app_discovery.rs:122-123`) carries the comment "Filtered out
   (background app) — silently skip." — describing filtering that
   does not exist. This looks like the filter was removed (or never
   added) while signature and comments stayed.

3. **`is_in_applications_dir` matches substrings.** The check is
   `path.to_string_lossy().contains("/Applications/")`
   (`app_discovery.rs:62-64`). Deliberate for `/Applications/`,
   `/System/Applications/`, `~/Applications/` — but it also matches:
   - helper bundles *nested inside* an installed app, e.g.
     `/Applications/Foo.app/Contents/Library/LoginItems/FooHelper.app`
     (Spotlight does index such nested bundles), which are exactly
     the "UI-less helpers" the trait contract excludes; and
   - any unrelated path containing the segment, e.g.
     `~/Downloads/Applications/Whatever.app`.

## Impact

The launcher's app list can contain non-launchable background
helpers (which typically show no icon and do nothing sensible when
launched), duplicated entries for apps shipping embedded helper
apps, and apps from directories that are not application
directories. Cosmetic in the best case, confusing launches in the
worst.

## Suggested fix

- In `try_discover_app`, return `Ok(None)` when the plist has
  `LSUIElement` or `LSBackgroundOnly` set to true (both may be
  bool `true` or string `"1"` in real-world plists — handle both),
  making the existing `Ok(None)` arm and its comment true.
- Exclude bundles nested inside another `.app` (e.g. reject paths
  whose parent chain contains a second `.app` component) or anchor
  the directory check to known roots instead of a substring match.
- If the trait-level contract is instead relaxed, update both
  comment sites so contract and code agree.
