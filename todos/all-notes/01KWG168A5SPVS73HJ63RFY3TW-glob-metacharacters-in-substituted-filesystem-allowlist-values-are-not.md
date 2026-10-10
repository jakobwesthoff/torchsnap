---
title: "Glob metacharacters in substituted filesystem allowlist values are not escaped"
kind: bug
component: gadget-host
impact: low
tags: [capabilities]
---
# Glob metacharacters in substituted filesystem allowlist values are not escaped

The filesystem allowlist substitutes `${...}` variables into a manifest pattern and then compiles the whole result as a glob, so glob metacharacters in the substituted values are read as glob syntax. A home directory with `?[]{}` makes the gadget fail to enable with an error that blames the manifest, and a `*` widens the allowlist silently.

## Problem
Read in the code, not reproduced.

`compile_fs_patterns` substitutes `${...}` variables into the
pattern string and then compiles the *whole* result as a glob
(`src-tauri/src/caps/filesystem.rs:185-224`). The values that
substitution inserts (home directory, xdg dirs, gadget-data path
all deriving from user-account/OS-level paths) are spliced in
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
platforms in scope, which is why this is filed as low impact,
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
Not gadget-exploitable. Security severity: nil / informational.
The correctness impact stays low as originally filed.

The allowlist widening via `*` triggers only when a *substituted
variable value* contains `*`, and every substitution value is
host/user/OS-derived. A malicious gadget chooses only *which*
variable to reference, never the value:

- `${home}` / `${xdg-config}` / `${xdg-data}` resolve from Tauri's
  path resolver (`home_dir()`/`config_dir()`/`data_dir()`,
  `src-tauri/src/gadget_host.rs:240-245`), which is pure OS/user state.
- `${gadget-data}` = `app_data_dir.join("gadget-home").join(gadget_id)`
  (`src-tauri/src/gadget_host.rs:251`), where `gadget_id` is the validated manifest
  `GadgetId`: non-empty, lowercase ASCII alphanumeric plus hyphen, no
  leading/trailing hyphen (`src-tauri/src/wasm/manifest/mod.rs:174-204`). No
  metacharacters are possible.
- `${gadget-archive}` = `source.root_path()`, the on-disk archive path
  (`src-tauri/src/wasm/source.rs:488-490`). The claim is stronger than "the id is
  clean": the install flow renames the archive to
  `<app_data_dir>/gadgets/<gadget_id>.torchsnap` using the validated id
  (`src-tauri/src/gadget_install/archive_ops.rs:26-60`, `publish_fresh` and
  `publish_replace`). A malicious author who ships
  `evil*.torchsnap` therefore gets normalized to `evil-gadget.torchsnap` on
  install, and the distributed filename never reaches the substitution.
  Native gadgets have a `None` source path, which becomes empty. They are
  trusted host code anyway.

A literal `*`/`**` in a manifest pattern is the gadget author's own
declared allowlist entry, visible in the manifest text the user
consents to at install (`src-tauri/src/wasm/manifest/permissions/filesystem.rs:20-22`). That is
the declared grant surface by design, not injection. Traversal in patterns
is rejected at parse (`src-tauri/src/wasm/manifest/permissions/filesystem.rs:85-93`), and request
paths reject `..`/`.`/`//`/NUL/relative paths and are canonicalized before
glob matching (`filesystem.rs:300-349`, which also closes symlink
escape).

Residual angles all fail closed or are host-side only. The
post-substitution check rejects `?[]{}`, so a host path containing those
fails the *load* (fail-closed), leaving only `*` to pass silently (the
documented robustness case). `to_string_lossy` can only introduce
U+FFFD, never a metacharacter. `canonicalize_pattern` output could
contain `*` only if an OS symlink resolves into a component literally
named `*` (host filesystem state, not gadget-reachable). Tokenizer
edge cases produce unrecognized or unterminated-variable errors, never
synthesized metacharacters. `literal_separator(true)` confines any
hypothetical injected `*` to a single path segment, so it cannot become
`**` or cross `/`. The only non-gadget path is a user manually
hand-placing a `we*rd.torchsnap` into `<app_data_dir>/gadgets/`
(bypassing the install command's filename normalization). That
requires the user to perform arbitrary filesystem operations in app
data on an attacker's instruction, a trust level at which the glob is
moot, and even then the widening is capped to sibling files in that one
directory. Negligible.

Two small robustness notes for this functional todo, surfaced by the
assessment: (1) the `compile_fs_patterns` doc comment
(`filesystem.rs:181-184`) claims the validator rejects `.` segments,
but `validate_fs_pattern` only rejects `..`. This is harmless: a `.` is
normalized away by `canonicalize_pattern`, and canonicalized request
paths never contain `.`, so the worst case is a fail-closed non-matching
pattern. The comment is worth a one-line correction. (2) Manually side-loaded
archives or directories with metacharacter names bypass the install-time
filename normalization (as above).
