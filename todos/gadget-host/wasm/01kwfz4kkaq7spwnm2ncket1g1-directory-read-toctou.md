---
kind: improvement
severity: low
status: open
area: [src-tauri/src/wasm/source.rs, src-tauri/src/wasm/path_safety.rs]
tags: [security, performance]
---

# DirectorySource reads: residual check-then-read race and per-read root canonicalization

Every `DirectorySource` read (`read_file`, `file_exists`, and the
manifest read in `open`) resolves its path through
`path_safety::existing_under_root`: the path is canonicalized, must
lie under the canonical root, and a path that does not exist is
reported "not found" with no second filesystem access. Two items
remain.

## 1. Residual race between the check and the read

Between `existing_under_root` canonicalizing the path and
`fs::read(&canonical)`, a concurrent writer running as the same user
could replace a directory component of the canonical path with a
symlink, so the read follows it out of the root. A lexical re-check
cannot close this. Closing it needs `openat2(RESOLVE_BENEATH)`,
per-component `O_NOFOLLOW`, or holding an `O_PATH` fd across check
and read.

Threat model, from the 2026-07-02 security pass:

- `DirectorySource` backs dev gadgets and directory-form gadgets only.
  User gadgets installed in the app are `.torchsnap` archives
  (`ArchiveSource`, an exact zip lookup with no symlink following).
  The installer only writes archives; a directory-form user gadget
  needs a same-user actor to place the directory.
- No privilege boundary is crossed: winning the race needs a
  concurrent same-user writer, which can already read anything the
  host could read. The only gain is handing outside bytes to a
  sandboxed guest.
- The guest-side trigger is ungated: `assets::read` / `assets::exists`
  pass a guest-chosen path into `read_file` / `file_exists`.
- A gadget with a file-creating command rule can plant the symlink in
  its own root, since the command `cwd` is unvalidated
  (`../caps/01kwh4j9bptrayf451yzd2145e-command-cwd-unvalidated.md`).
  That still needs the race and still crosses no privilege boundary.

Decide whether to accept this residual and record that, or close it
with fd-relative opens.

## 2. Root canonicalized on every read

`existing_under_root` canonicalizes the gadget root on every call,
a syscall per asset request on the asset-serving path, although the
root does not change after `open()`. Caching the canonical root in
`DirectorySource` at `open()` time would avoid it; the shared helper
would then need a variant that takes an already-canonical root.
