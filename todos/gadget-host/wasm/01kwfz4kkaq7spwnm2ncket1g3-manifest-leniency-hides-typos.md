---
kind: improvement
severity: medium
status: open
area: [src-tauri/src/wasm/manifest/mod.rs]
---

# Manifest parse leniency silently hides author mistakes

## Problem
Three leniency choices in manifest parsing combine to make gadget
author typos fail silently or confusingly:

1. Unknown fields are accepted everywhere. This is intentional
   forward compatibility (test
   `ignore_unknown_fields_in_gadget_section`,
   `src-tauri/src/wasm/manifest/mod.rs:799-815`), but it also
   means `prefixs = ["!"]`, `[frontend.veiws]`, or a
   `[permission]` (singular) table are silently dropped: the
   gadget loads with the feature missing and no diagnostic.
2. Icon strings without the exact `heroicons:` prefix fall
   through to the `Asset` path variant
   (`mod.rs:240-257`). A typo like `heroicon:beaker` becomes an
   asset path and surfaces later as a confusing
   "reading gadget file `heroicon:beaker`" load error, or as a
   missing icon, depending on the caller.
3. `version` is a free-form string (`mod.rs:120-122`); the doc
   comment says it is intended for "future update checking",
   which will require an ordering. Nothing rejects
   `version = "latest"` today, and retrofitting semver
   validation later will break already-shipped manifests.
4. `storage.sql.migrations` entries are documented as "should
   resolve to a `.sql` text file"
   (`manifest/storage.rs:41-47`) but only path safety is
   validated; a manifest pointing a migration at
   `frontend/launcher.js` parses fine and fails later when the
   bytes are applied as SQL.

## Impact
Gadget authors lose time debugging silently-ignored manifest
sections; the eventual update-checker inherits unvalidatable
version strings.

## Suggested fix
- Collect unknown keys during parse (serde `flatten` catch-all
  per section, or a post-parse walk of the raw
  `toml::Value`) and log a warning naming them; a hard
  `deny_unknown_fields` would sacrifice forward compatibility,
  a warning does not.
- Validate icon asset paths end in a known image extension at
  parse time so prefix typos are caught with a pointed error.
- Decide now whether `version` must be semver, and validate if
  so.
