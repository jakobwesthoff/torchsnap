---
title: "Filesystem requests leak path existence, canonical paths and OS errors outside the allowlist"
kind: bug
component: gadget-host
impact: low
tags: [security, capabilities]
---
# Filesystem requests leak path existence, canonical paths and OS errors outside the allowlist

`resolve_request` canonicalizes the path before it checks the allowlist, and it returns a different error depending on what canonicalization hit. A gadget can therefore tell absent paths from present but denied ones, read back real home and symlink paths from the denial message, and read OS error text. The same order also leaks existence inside an allowed symlink's target. The gadget must already hold a read grant, so this is reconnaissance only, and it is low in impact.

## Problem
`resolve_request` (`src-tauri/src/caps/filesystem.rs:331-349`) does the
existence-revealing `std::fs::canonicalize(path)` *before* consulting
the allowlist, and returns a **different `FilesystemError` variant
depending on what canonicalize hit**, even for paths the gadget has no
grant on:

```rust
validate_request_path(path)?;                        // invalid-path (uniform)
let canonical = match std::fs::canonicalize(path) {
    Ok(c) => c,
    Err(e) if e.kind() == NotFound => return Err(NotFound),
    Err(e) => return Err(Io(e.to_string())),
};
if !allowlist.globset.is_match(&canonical) { return Err(PermissionDenied(canonical...)); }
Ok(canonical)
```

The variant survives the WIT bridge (`src-tauri/src/wasm/runtime/host/fs.rs:23-33`)
as a distinct `fs-error` case, so the guest observes it through
`read_file` and `metadata`. (`file_exists`, `:128-130`, collapses to
`bool` and is not an oracle, but a gadget just uses
`read_file` or `metadata` instead.)

## Impact

### Exact leak enumeration (path *outside* the allowlist)
The WIT doc (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit:374-391`) itself spells out three
distinguishable outcomes, and their messages carry payload back to the
guest. Invalid syntactic paths (`..`, `.`, `//`, NUL) are rejected
uniformly as `invalid-path` before any I/O, and they leak nothing. For a
syntactically clean absolute path:

1. **`not-found`**: some component of the path is absent (the classic
   existence bit).
2. **`permission-denied(msg)`**: the *fully resolved* path exists
   (canonicalize succeeded) but is not allowlisted. There are two sub-leaks. The first is
   the existence bit, since every component exists and is searchable. The second is that
   **`msg` is the post-canonicalization path** (`:344`, documented at
   WIT `:377`), which leaks the user's real home directory and any
   symlink resolution along the way (`/var` to `/private/var`, or a `~/foo`
   symlink's real target). It confirms existence and discloses canonical
   layout.
3. **`io(msg)`**: canonicalize failed with something other than
   `NotFound` (`:339`), and **`msg` is the raw OS error string** (WIT
   `:386-390`). This is the most overlooked leak. `EACCES` on a directory
   component means the path exists but an ancestor is not world-searchable.
   Probing under `/root/...` or another user's `0700` home confirms
   those dirs exist and that the app runs unprivileged. `ENOTDIR` means an
   intermediate component is a regular file. `ELOOP` means a symlink cycle
   exists.

So the realized oracle is richer than binary: **absent** vs
**present-but-denied with canonical path disclosed** vs
**present-but-unresolvable with OS errno text**. A gadget can enumerate
installed apps (`/Applications/1Password.app`), security tooling,
credential and dotfile presence (`~/.aws/credentials`, `~/.ssh/id_*`,
browser profiles, wallet DBs), other gadgets' data dirs, and read back
real home and symlink paths from the denial message.

The `metadata` `symlink_metadata` pre-check (`:136-138`) leaks nothing
extra. Its `is_symlink` result is discarded on every error path. It is a
correctness note only: it reports whether the path the gadget *spelled*
is a symlink, not the canonical target, which matches the WIT contract
at `:396-398`.

### A symlink-escape existence leak the current code gets wrong
Beyond enumeration of arbitrary paths, the current order leaks
existence *inside an allowed symlink's target dir*. With
`/allowed/link -> /outside` (existing) and request `/allowed/link/secret`,
today `secret` present gives `permission-denied`, and `secret` absent gives
`not-found`. That leaks the existence of `/outside/secret`. The fix below
closes this too.

### Severity: low (top of range)
The gadget must already hold a `[permissions.filesystem]` read grant.
It already reads *some* files, so the bug grants no new read power. It
is recon-only. No file content leaks, and no code executes. The output is
metadata: existence, canonical layout, and component permission and type
state. Its value is realized only when chained. Enumeration is a
targeting primitive for a *second* capability (network exfil, command
execution), or a way to justify a broader grant to the user. That chaining
requirement keeps it out of medium. What pushes it to the top of low
is that the `io` and `permission-denied` message channels leak more than existence
(errno state of arbitrary components, plus real resolved home and symlink
paths), and credential-file and installed-software fingerprinting is
genuinely useful. It is formalized as **low**. Low-medium is defensible if
the rubric weighs "meaningfully improves a realistic attack chain."

Residual channel the fix does not close: **timing.** `canonicalize`
does real I/O, so even with unified errors a gadget can time responses to
infer existence. This is a weak side channel, out of scope, and noted as a known
limitation.

## Suggested fix
Goal: for a path not under any allowlisted root, return **one
indistinguishable error (`permission-denied`) with a payload that
reveals nothing new**, whether or not the leaf exists. Preserve
`not-found` *only within* the gadget's own allowlisted roots, which is
the legitimate "does my allowed file exist" case. Do not reintroduce a
symlink escape.

Key idea: decide allowlist membership on the path's *effective
canonical form*. Canonicalize the **deepest existing ancestor**
(resolving symlinks in every existing component), and append the
guaranteed-nonexistent trailing components lexically. Because
`validate_request_path` already rejected `..` and `.`, and nonexistent
components cannot be symlinks, the lexical tail is safe. The allowlist
gate runs on that effective path *before* the leaf's existence is
surfaced.

```
validate_request_path(request)?                 // unchanged: invalid-path

let mut suffix: Vec<OsString> = vec![];
let mut cur: &Path = Path::new(request);
let base: PathBuf = loop {
    match std::fs::canonicalize(cur) {
        Ok(base) => break base,                 // resolves all symlinks in the prefix
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let Some(name)   = cur.file_name() else { return deny(request) };
            let Some(parent) = cur.parent()    else { return deny(request) };
            suffix.push(name.to_os_string());
            cur = parent;
        }
        // EACCES / ENOTDIR / ELOOP / anything non-NotFound: cannot resolve
        // through `cur`, so cannot prove the path is inside an allowed root,
        // and must not treat an unresolvable component lexically (could be a
        // symlink -> escape). Deny uniformly; never surface the OS error.
        Err(_) => return deny(request),
    }
};

let mut effective = base;
for name in suffix.iter().rev() { effective.push(name); }

if !allowlist.globset.is_match(&effective) {
    return deny(request);                       // out-of-allowlist: same whether leaf exists or not
}
if !suffix.is_empty() {
    return Err(FilesystemError::NotFound);       // allowed root, leaf missing
}
Ok(effective)                                    // fully exists AND allowed

// deny() => FilesystemError::PermissionDenied(request.to_string())
//           echo the RAW request string, never the resolved canonical.
```

Why each piece matters:
- Fully existing paths are unchanged. The first iteration is
  `canonicalize(full_path)`, and on `Ok` the suffix is empty and
  `effective == canonical`. Existing passing tests keep passing.
- The existence oracle is closed. An out-of-allowlist path returns
  `permission-denied` whether the leaf exists (first `Ok`, unmatched)
  or is missing (walk up, unmatched). The result is uniform.
- The symlink escape via a nonexistent leaf is closed. For
  `/allowed/link -> /outside` and request `/allowed/link/secret`, both a present and an
  absent leaf walk to `base=/outside`, `effective=/outside/secret`, unmatched,
  giving `permission-denied`. The result is uniform.
- The `io` leak is eliminated for the gate. Non-`NotFound` canonicalize errors
  (`EACCES`, `ENOTDIR`, `ELOOP`) become `permission-denied`. Deny-by-
  default is the only *safe* choice, since walking past an unresolvable
  component and appending it lexically would risk treating an
  unresolved symlink as a plain name (an escape).
- The message payload is neutered. `deny()` echoes the **raw request string**
  the gadget already supplied, not the resolved canonical path. A canonical
  message would re-leak existence and disclose home and symlink layout.
- **Do NOT use `Path::exists()` for the ancestor walk.** `exists()`
  follows symlinks and returns `false` on any error including
  `EACCES` and `ELOOP`. That conflates "unresolvable" with "absent", and it lets
  an unresolved symlink component be appended lexically, which reintroduces
  the escape. The compile-time helper `canonicalize_pattern`
  (`filesystem.rs:271`) uses an `.exists()` walk on *trusted* manifest patterns at
  load. That is lower risk, but it is the same latent pattern, so flag it as a
  follow-up hardening note.
- Branch only on `NotFound` versus everything else. This avoids any dependency on the newer
  `ErrorKind::NotADirectory` or `FilesystemLoop` and is portable. `file_name()`
  or `parent()` returning `None` means the walk reached the filesystem root
  without an existing ancestor (pathological), so deny.

Doc changes to accompany the fix (`torchsnap-gadget.wit`): update
`permission-denied` (`:375-378`). The message is now the request path as
supplied, and it no longer implies the path "exists", since it covers
exists-but-denied and missing-outside-allowlist alike. Update `io`
(`:386-390`): it no longer covers canonicalization failures, only
post-gate read and metadata failures on *allowed* files. Correct the
call-time contract (`:355-361`), which currently documents the
vulnerable canonicalize-then-match order.

Post-gate residual (minor, optional): `read_file` (`:117-120`) and
`metadata` (`:141-144`) still map non-`NotFound` I/O to `io(e.to_string())`,
echoing OS strings. That path runs only on *allowed* files, so it
is not the oracle. Masking those too is cheap defense in depth for zero
OS-string egress.

## Tests to add
1. Oracle closure (core regression): an existing out-of-allowlist path
   and a nonexistent out-of-allowlist path both return
   `PermissionDenied`.
2. `io` masking: an `ELOOP` (mutual symlinks `a -> b`, `b -> a`) and an
   `EACCES` (`0000` ancestor dir) outside the allowlist should assert
   `PermissionDenied`, not `Io`.
3. Message neutrality: the `PermissionDenied` payload equals the raw
   request string and does not contain the resolved canonical path (e.g. no
   `/private/var` for a `/var/...` probe).
4. Nonexistent-but-allowed still gives `NotFound` (add a `**` or multi-segment
   variant beyond the existing simple case).
5. Symlink escape with a nonexistent leaf: `/allowed/link -> /outside`,
   request `/allowed/link/missing` gives `PermissionDenied`, plus the
   existing-leaf twin, both `PermissionDenied`.
6. Depth note: a deep nonexistent path issues O(components)
   `canonicalize` syscalls, bounded by `PATH_MAX`. This is not a real DoS.
   Mention it as a known cost rather than adding a cap, or add an explicit
   component limit if desired.

## Files
`src-tauri/src/caps/filesystem.rs` (`resolve_request` `:331-349` fix site; tests
`:355-678`; `canonicalize_pattern` `.exists()` walk `:271`),
`src-tauri/src/wasm/runtime/host/fs.rs:23-33` (bridge), `gadgets/gadget-sdk/wit/torchsnap-gadget.wit:355-422`
(contract to update).
