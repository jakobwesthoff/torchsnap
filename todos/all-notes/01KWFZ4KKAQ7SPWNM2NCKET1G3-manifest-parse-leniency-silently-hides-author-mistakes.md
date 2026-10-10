---
title: "Manifest parse leniency silently hides author mistakes"
kind: improvement
component: gadget-host
---
# Manifest parse leniency silently hides author mistakes

Leniency in manifest parsing makes gadget author typos fail silently or confusingly. Authors lose time debugging sections the parser silently ignored, and the eventual update checker inherits version strings it cannot validate.

## Problem
1. Unknown fields are accepted everywhere. This is intentional forward compatibility (test `ignore_unknown_fields_in_gadget_section` in `src-tauri/src/wasm/manifest/mod.rs`, around lines 799 to 815). It also means `prefixs = ["!"]`, `[frontend.veiws]`, or a `[permission]` (singular) table are silently dropped: the gadget loads with the feature missing and no diagnostic.
2. Icon strings without the exact `heroicons:` prefix fall through to the `Asset` path variant (`src-tauri/src/wasm/manifest/mod.rs`, around lines 240 to 257). A typo like `heroicon:beaker` becomes an asset path. It then surfaces later as a confusing "reading gadget file `heroicon:beaker`" load error, or as a missing icon, depending on the caller.
3. `version` is a free-form string (`src-tauri/src/wasm/manifest/mod.rs`, around lines 120 to 122). The doc comment says it is intended for "future update checking", which will require an ordering. Nothing rejects `version = "latest"` today, and retrofitting semver validation later will break already-shipped manifests.
4. `storage.sql.migrations` entries are documented as "should resolve to a `.sql` text file" (`src-tauri/src/wasm/manifest/storage.rs`, around lines 41 to 47), but only path safety is validated. A manifest pointing a migration at `frontend/launcher.js` parses fine and fails later, when the bytes are applied as SQL.

## Impact
Gadget authors lose time debugging silently ignored manifest sections. The eventual update checker inherits version strings it cannot validate.

## Proposal
- Collect unknown keys during parse (a serde `flatten` catch-all per section, or a post-parse walk of the raw `toml::Value`) and log a warning naming them. A hard `deny_unknown_fields` would sacrifice forward compatibility; a warning does not.
- Validate icon asset paths end in a known image extension at parse time, so prefix typos are caught with a pointed error.
- Decide now whether `version` must be semver, and validate it if so.
