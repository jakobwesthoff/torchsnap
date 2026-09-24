---
kind: bug
severity: low
status: open
area: [src-tauri/src/wasm/manifest/permissions/opener.rs]
tags: [unconfirmed]
---

# Opener schemes are not validated or normalized

## Problem
`OpenerPermissionsDef::validate`
(`src-tauri/src/wasm/manifest/permissions/opener.rs:65-74`) only
checks that *some* capability is granted. The `schemes` entries
themselves are accepted verbatim:

- `schemes = [""]` counts as "a capability was granted" (the
  list is non-empty) yet grants nothing meaningful.
- `schemes = ["HTTPS"]` is stored uppercase; URL schemes are
  case-insensitive per RFC 3986 but compare case-sensitively as
  plain strings, so whether this grant ever matches depends on
  how the runtime check normalizes the URL side.
- Duplicates and whitespace-containing entries pass.

Contrast `[permissions.http]`, which normalizes origins at parse
time precisely so runtime checks are plain string equality
(`permissions/http.rs:49-81`). Opener schemes get no equivalent
treatment.

## Impact
An author writing `["HTTPS"]` or `["https://"]` may ship a grant
that silently never matches (or matches inconsistently),
surfacing as "open-url rejected" at runtime with no hint the
manifest entry is the problem.

## Suggested fix
Validate each scheme against the RFC 3986 scheme grammar
(`ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`), reject empty
entries, and lowercase-normalize at parse time, mirroring the
http origin approach. Add parse tests.
