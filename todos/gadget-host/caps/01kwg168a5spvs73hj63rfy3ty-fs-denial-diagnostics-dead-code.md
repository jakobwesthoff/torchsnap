# FsAllowlist retains canonical_patterns for diagnostics that don't exist

**Kind:** refactor
**Severity:** low
**Area:** src-tauri/src/caps/filesystem.rs

## Problem
`FsAllowlist` carries a `canonical_patterns: Vec<String>` field
whose doc comment says it is "retained only for diagnostics
(e.g. logging which patterns a denied request was checked
against)" (`src-tauri/src/caps/filesystem.rs:81-92`). The field
is `#[allow(dead_code)]` and nothing in the codebase reads it
except one test asserting it is non-empty
(`filesystem.rs:676`).

The diagnostics the comment describes were never built: a denied
request produces `FilesystemError::PermissionDenied(<canonical
path>)` (`filesystem.rs:342-345`) with no mention of the
patterns consulted, and no log entry is emitted anywhere in the
deny path.

## Impact
When a gadget author's read is denied, the only signal is
"path not permitted: /the/path" — they cannot see what the
compiled allowlist actually contains (which matters, because
compilation canonicalizes prefixes, e.g. `/var` becoming
`/private/var`, and substitutes variables). Debugging manifest
patterns requires reading host source. Meanwhile the field
costs memory per gadget and the comment promises behavior that
doesn't exist.

## Suggested fix
Either build the promised diagnostics — include (or debug-log)
the compiled `canonical_patterns` when returning
`PermissionDenied` — or delete the field and the claim. The
former is genuinely useful for gadget development and the data
is already there.
