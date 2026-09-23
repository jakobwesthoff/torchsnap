# Built-in gadgets request broader opener grants than they use

**Kind:** improvement
**Severity:** low
**Area:** src-tauri/src/gadgets/app_launcher.rs, src-tauri/src/gadgets/system_preferences.rs

## Problem

Both built-in catalog gadgets request the wildcard URL-scheme
grant although neither needs it:

- `AppLauncherGadget::cap_requests`
  (`app_launcher.rs:56-66`) requests
  `schemes: vec!["*".into()]` plus `open_path`/`reveal_path`. The
  gadget only ever calls `opener.open_path` and
  `opener.reveal_path` (`app_launcher.rs:236-250`); no
  `open_url` call exists in the file.
- `SystemPreferencesGadget::cap_requests`
  (`system_preferences.rs:45-56`) requests
  `schemes: vec!["*".into()]` but only opens
  `x-apple.systempreferences:` URLs
  (`system_preferences.rs:160-166`).

The opener capability treats `*` as a short-circuit that passes
raw, unparsed strings straight to the opener backend (see
`src-tauri/src/caps/opener.rs:129-131` and the parked security
queue entry on opener scheme wildcards). Built-ins are trusted
code, so this is not an exploit path by itself, but they set the
reference example for what a "normal" permission request looks
like, and the wildcard weakens any future auditing/consent UI
story ("built-ins ask for `*` too").

## Suggested fix

- App launcher: drop `schemes` entirely (empty vec) — it opens
  paths, not URLs.
- System preferences: request
  `schemes: vec!["x-apple.systempreferences".into()]` (adjust for
  however non-macOS backends express their pane URLs when those
  arrive).
