---
kind: bug
severity: low
status: open
area: [src-tauri/src/caps/filesystem.rs]
tags: [unconfirmed]
---

# Filesystem allowlist: glob metacharacters in substituted variable values are not escaped

## Problem
`compile_fs_patterns` substitutes `${...}` variables into the
pattern string and then compiles the *whole* result as a glob
(`src-tauri/src/caps/filesystem.rs:185-224`). The values that
substitution inserts (home directory, xdg dirs, gadget-data path
— all deriving from user-account/OS-level paths) are spliced in
verbatim, so any glob metacharacter that happens to occur in a
real directory name is interpreted as glob *syntax*, not as a
literal character:

- `?`, `[`, `]`, `{`, `}` in a substituted value trip the
  unsupported-metacharacter check at `filesystem.rs:201-210` and
  make the gadget fail to enable with an error blaming the
  *manifest pattern*, even though the pattern itself is fine.
- `*` in a substituted value is NOT caught by that check (only
  `?[]{}` are rejected; `*` is the supported wildcard). It
  compiles into a wildcard, silently *widening* the allowlist:
  a home directory `/Users/we*rd` yields a compiled pattern
  `/Users/we*rd/...` that matches `/Users/weird/...`,
  `/Users/we-anything-rd/...`, etc.

The same applies to `canonicalize_pattern`'s output: the
canonicalized static prefix (`filesystem.rs:246-295`) is
re-joined with the glob suffix and compiled without escaping.

Directory names with glob metacharacters are rare on the
platforms in scope, which is why this is filed as low severity —
but nothing in the system rules them out (macOS allows `*` in
folder names, and users do put app data in odd paths).

## Impact
- Spurious gadget enable failures with a misleading error message
  for users whose paths contain `?[]{}`.
- Allowlist widening (more paths readable than the manifest
  declared) for paths containing `*`. This is a
  permission-boundary concern; see the exploitability assessment
  below, which found it not gadget-exploitable.

## Suggested fix
Escape glob metacharacters in everything that is *data* rather
than pattern syntax: apply `globset::escape` to each substituted
variable value at substitution time (requires substituting via a
callback or marker rather than plain string replace), or escape
the canonicalized static prefix before re-joining it with the
glob suffix. Add tests with metacharacter-bearing directory
names.

## Security-pass exploitability assessment (2026-07-02)
**Not gadget-exploitable. Security severity: nil / informational.**
The correctness severity stays low as originally filed.
The allowlist-widening via `*` triggers only when a *substituted
variable value* contains `*`, and every substitution value is
host/user/OS-derived — a malicious gadget chooses only *which*
variable to reference, never the value:

- `${home}` / `${xdg-config}` / `${xdg-data}` resolve from Tauri's
  path resolver (`home_dir()`/`config_dir()`/`data_dir()`,
  `gadget_host.rs:240-245`) — pure OS/user state.
- `${gadget-data}` = `app_data_dir.join("gadget-home").join(gadget_id)`
  (`gadget_host.rs:251`), where `gadget_id` is the validated manifest
  `GadgetId` — non-empty, lowercase ASCII alphanumeric plus hyphen, no
  leading/trailing hyphen (`manifest/mod.rs:174-204`). No
  metacharacters possible.
- `${gadget-archive}` = `source.root_path()`, the on-disk archive path
  (`source.rs:488-490`). The claim is actually stronger than "id is
  clean": the install flow renames the archive to
  `<app_data_dir>/gadgets/<gadget_id>.torchsnap` using the validated id
  (`gadget_install.rs:150,164-166`), so a malicious author shipping
  `evil*.torchsnap` gets normalized to `evil-gadget.torchsnap` on
  install; the distributed filename never reaches the substitution.
  Native gadgets have `None` source path → empty (and are trusted host
  code anyway).

A literal `*`/`**` in a manifest pattern is the gadget author's own
declared allowlist entry, visible in the manifest text the user
consents to at install (`permissions/filesystem.rs:20-22`) — the
declared grant surface by design, not injection. Traversal in patterns
is rejected at parse (`permissions/filesystem.rs:85-93`), and request
paths reject `..`/`.`/`//`/NUL/relative and are canonicalized before
glob matching (`filesystem.rs:300-349`, which also closes symlink
escape).

Residual angles all fail closed or are host-side only: the
post-substitution check rejects `?[]{}` so a host path containing those
fails the *load* (fail-closed), leaving only `*` to pass silently (the
documented robustness case); `to_string_lossy` can only introduce
U+FFFD, never a metacharacter; `canonicalize_pattern` output could
contain `*` only if an OS symlink resolves into a component literally
named `*` (host filesystem state, not gadget-reachable); tokenizer
edge cases produce unrecognized/unterminated-variable errors, never
synthesized metacharacters; and `literal_separator(true)` confines any
hypothetical injected `*` to a single path segment (it cannot become
`**` or cross `/`). The only non-gadget path is a user manually
hand-placing a `we*rd.torchsnap` into `<app_data_dir>/gadgets/`
(bypassing the install command's filename normalization) — which
requires the user to perform arbitrary filesystem operations in app
data on an attacker's instruction, a trust level at which the glob is
moot, and even then the widening is capped to sibling files in that one
directory. Negligible.

Two small robustness notes for this (functional) todo, surfaced by the
assessment: (1) the `compile_fs_patterns` doc comment
(`filesystem.rs:181-184`) claims the validator rejects `.` segments,
but `validate_fs_pattern` only rejects `..` — harmless (a `.` is
normalized away by `canonicalize_pattern` and canonicalized request
paths never contain `.`, so worst case is a fail-closed non-matching
pattern), worth a one-line correction; (2) manually side-loaded
archives/directories with metacharacter names bypass the install-time
filename normalization (as above).
