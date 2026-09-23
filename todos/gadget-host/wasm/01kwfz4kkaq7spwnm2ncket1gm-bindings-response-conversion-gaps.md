# Response conversion gaps: silent JSON drop, Nothing conflation, raw icon URLs

**Kind:** possible-bug
**Severity:** low
**Area:** src-tauri/src/wasm/bindings.rs

## Problem
Three small semantics gaps in the WIT→native response
conversion:

1. **Malformed `data` JSON is silently dropped.**
   `parse_optional_json`
   (`src-tauri/src/wasm/bindings.rs:261-263`) maps a gadget's
   unparseable `view-response.data` string to `None` with no
   diagnostic. Contrast the asset-icon resolver in the same
   file, which returns warnings that the bridge logs. A gadget
   author whose custom UI mysteriously receives no data gets
   zero feedback.
2. **`SearchResponse::Nothing` is conflated with
   `Results([])`** (`:236-241`). The WIT gives guests two
   distinct variants; the host converts both to
   `GadgetResponse::Results(vec![])`. Per the bridge comment
   (`bridge.rs:683-686`), empty results actively evict stale
   frontend entries, so a guest returning `Nothing`
   ("not participating") behaves identically to "participating
   with zero hits". If that equivalence is intended, the WIT
   `nothing` variant is redundant and should say so in its doc;
   if not, `Nothing` should map to the bridge returning `None`.
3. **Relative `AssetIcon` paths are spliced into URLs without
   validation or encoding** (`:339-342`):
   `format!("torchsnap-gadget://localhost/{gadget_id}/{path}")`.
   A path containing spaces, `#`, or `?` produces a URL whose
   fetch will fail or truncate; nothing runs
   `validate_gadget_path` here (rejection currently relies on
   the source layer at serve time). Percent-encode the path
   segments when building the URL (interacts with the protocol
   percent-decoding todo,
   `01kwfz4kkaq7spwnm2ncket1fv-protocol-no-percent-decoding.md`).

## Impact
All three degrade silently: missing custom-UI data, stale-entry
eviction differences, and broken icons, each without a log line
pointing at the cause.

## Suggested fix
Return a warning from `parse_optional_json` failures like the
icon resolver does; decide and document the `Nothing` semantics;
validate + percent-encode asset icon paths at rewrite time.
