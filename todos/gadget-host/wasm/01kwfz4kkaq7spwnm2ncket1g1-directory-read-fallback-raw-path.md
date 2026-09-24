---
kind: bug
severity: low
status: open
area: [src-tauri/src/wasm/source.rs]
tags: [unconfirmed]
---

# DirectorySource::read_file missing-file branch reads via raw path

## Problem
`DirectorySource::read_file` handles the missing-file case
(`resolve_inside_root` returned `None`) by re-reading with the
raw, unnormalized caller path
(`src-tauri/src/wasm/source.rs:212-219`):

```rust
None => {
    // Surface a proper "not found" error. ...
    std::fs::read(self.root.join(path))
        .with_context(|| format!("reading gadget file `{path}`"))
}
```

Two inconsistencies:

1. `validate_gadget_path` returns a normalized path
   (`source.rs:376`), and the archive backend performs its
   lookup with that normalized form (`:451-457`). The directory
   fallback ignores it and joins the raw string, so the two
   backends resolve dot-segment paths through different strings.
   Today the outcome is the same because the branch is only
   reached when the file does not exist, but the branch performs
   a real `fs::read` whose result is returned on success.
2. `resolve_inside_root` canonicalizes the gadget root on every
   single read (`:184-187`), an avoidable per-request syscall
   since the root never changes after `open()`; it could be
   canonicalized once and cached.

## Impact
Functionally invisible in the common case (the fallback read
fails with NotFound, which is the intended error). The branch is
nonetheless a second read path with weaker guarantees than the
primary one, and the per-read root canonicalization is wasted
work on the asset-serving hot path.

## Suggested fix
Have the `None` branch construct the NotFound error directly
(no second `fs::read`), or read via the normalized path if a
read is kept. Cache the canonical root in the struct at
`open()` time. The time-of-check gap between canonicalize
failure and the fallback read also has a security angle,
assessed below.

## Security-pass assessment (2026-07-02)
Confirmed real; severity **low** under the actual threat model.

The asymmetry: `resolve_inside_root` returns `Ok(None)` whenever
`full_path.canonicalize()` fails, and `read_file`'s `None` branch
then performs an unguarded `std::fs::read(self.root.join(path))`
that follows symlinks with no `canonicalize`+`starts_with` check.
It is the only read in `DirectorySource` that bypasses the
symlink-target guard; the `Some` branch is safe (it reads the
already-canonicalized, root-confined path).

**There is no non-racy variant — this bounds the severity.** A
symlink only reaches the `None` branch when its fully-resolved
target is *absent at canonicalize time* (that is what makes
`canonicalize()` fail). The three cases, verified empirically on
macOS with `std::fs`:

| In-dir symlink | `canonicalize()` | `fs::read()` | Outcome |
|---|---|---|---|
| target exists (`→ /etc/passwd`) | `Ok(/private/etc/passwd)` | would read | `Some` branch → `starts_with(root)` false → rejected "escapes" |
| target stays absent (dangling) | `Err(NotFound)` | `Err(NotFound)` | `None` branch reads nothing — no leak |
| absent at check, created before read | `Err(NotFound)` → `None` | `Ok(external bytes)` | `None` branch leaks the external file |

So the leak window is exactly "absent at check, present at read",
which is definitionally a race. A stable symlink to a path that
later becomes present does not yield a stable exploit: the next
call after the target exists canonicalizes successfully and is
caught by the `Some` branch. Other `realpath()` vs `open(2)`
divergences (EACCES on a component, ELOOP, ENAMETOOLONG,
intermediate-dir symlinks) fail both calls identically, so none
give a non-racy check/use split.

**Why low severity:**
- Unreachable for the normal user-gadget case: user gadgets ship
  as `.torchsnap` archives backed by `ArchiveSource`, whose
  `by_name` is an exact zip-namespace lookup with no filesystem
  symlink following. The bug is `DirectorySource`-only.
- `DirectorySource` backs dev gadgets (the developer's own trusted
  code) and directory-form gadgets.
- No privilege boundary is crossed. Torchsnap runs as the user; to
  plant the symlink and win the absent→present race you need a
  concurrent writer running as the same UID, which already has full
  read access to everything Torchsnap can read — so exfiltrating a
  file *through* Torchsnap yields nothing it couldn't read directly.
  The only theoretical value is a WASM-sandbox-confinement bypass
  (making the host hand external bytes to the confined guest), and
  even that needs a same-user native accomplice with strictly
  greater capability than the exploit returns, leaking a file that
  must transiently blink into existence in the microsecond window.
- The guest-side *trigger* is trivial and ungated: `assets::read` /
  `assets::exists` (`runtime/host/assets.rs`) pass a fully
  guest-controlled `path` straight into `read_file`/`file_exists`,
  and `assets` is available to every gadget. What is hard is
  satisfying the symlink+race precondition, not triggering the read.

A directory-form *user* gadget widens reachability only
marginally: the loader picks the source by `path.is_dir()`
(`lib.rs:1139-1147`) independent of `GadgetSourceKind`, so a
directory placed under `<app_data_dir>/gadgets/<id>/` loads as a
`User` gadget via `DirectorySource`. But the sanctioned in-app
installer only ever writes `<id>.torchsnap` archives
(`gadget_install.rs`), never a directory, so reaching a
directory-form user gadget still requires a same-user actor to
place the directory by other means. The same-user /
no-privilege-gain ceiling is unchanged.

**Fix (correct and sufficient for B6):** replace the `None`
branch's `std::fs::read(self.root.join(path))` with a
directly-synthesized not-found error and no second filesystem
read:

```rust
None => Err(std::io::Error::from(std::io::ErrorKind::NotFound))
    .with_context(|| format!("reading gadget file `{path}`")),
```

After this, every byte `DirectorySource::read_file` returns comes
from `fs::read(&canonical)` where `canonical` passed
`canonicalize()` + `starts_with(&canonical_root)`. The unguarded
read path is gone, fully closing the `None`-branch symlink-follow.
`file_exists`'s `None` arm already returns `Ok(false)` and needs
no change.

Test impact is clean: `reject_nonexistent_file` (`source.rs:778-802`)
requires the error to not contain "escapes" and to name the missing
file — the synthesized `NotFound` with the `format!("reading gadget
file \`{path}\`")` context satisfies both; `read_wasm_fails_if_wasm_file_missing`
(`:804-812`) only asserts `is_err()`; no test asserts the error is an
`io::Error` produced by an actual read syscall. Add a regression test
that a dangling in-dir symlink to an existing external file yields a
not-found error, not the external contents (the deterministic form of
the race).

`O_NOFOLLOW` is not needed for this fix (it would only bear on the
residual `Some`-branch race, and only for the final component) and
should not be added as part of B6. Caching the canonical root at
`open()` time is a security-neutral, optional perf cleanup.

**Residual (record as conscious acceptance, out of scope for the
one-line fix):** the `Some` branch still has the generic
canonicalize-then-open TOCTOU — between `full_path.canonicalize()`
and `fs::read(&canonical)`, a concurrent same-user writer could
replace a *directory component* of `canonical` with a symlink so
the `open` follows it out of root. This cannot be closed with a
lexical re-check; fully closing it needs `openat2(RESOLVE_BENEATH)`
/ per-component `O_NOFOLLOW`, or holding an `O_PATH` fd across
check and read. Given the same-user, `DirectorySource`-only threat
model with no privilege gain, this is very low priority but should
be acknowledged rather than silently left.

## Adjacent findings surfaced during this analysis
- **Command-`cwd` self-planting (already tracked).** A gadget
  holding a `[[permissions.command]]` rule for a file-creating
  binary (e.g. `ln -s`) plus `path_resolver` can, because
  `command::run`'s guest `cwd` is unvalidated, run with `cwd` set
  to its own `DirectorySource` root and plant the symlink itself —
  so B6's "concurrent writer" precondition can, in that permission
  configuration, be met by the gadget rather than an external
  process. This does not raise B6's severity (still needs the race,
  still crosses no privilege boundary, still yields nothing the
  spawned binary couldn't read directly). The cwd gap is the
  serious issue and is tracked in
  `../host-core/01kwh4j9bptrayf451yzd2145e-command-cwd-unvalidated.md`.
- **`DirectorySource::open` reads `manifest.toml` unguarded (new).**
  `DirectorySource::open` does a raw
  `std::fs::read_to_string(root.join("manifest.toml"))`
  (`source.rs:146-149`) with no `validate_gadget_path` /
  `resolve_inside_root` guard, so a symlinked `manifest.toml` is
  followed out of root on the next load. Filed separately:
  `01kwh4j9bptrayf451yzd2145w-directory-source-manifest-symlink-follow.md`.
