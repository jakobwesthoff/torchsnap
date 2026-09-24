---
kind: bug
severity: low
status: open
area: [src-tauri/src/wasm/source.rs]
tags: [unconfirmed]
---

# `DirectorySource::open` reads `manifest.toml` with a raw, unguarded `read_to_string` (symlink-follow out of root)

Surfaced during the B6 security-pass analysis
(`01kwfz4kkaq7spwnm2ncket1g1-directory-read-fallback-raw-path.md`),
not one of the originally-parked queue items.

## Problem
`DirectorySource::open` reads the manifest with a raw filesystem
read and no path guard (`source.rs:144-155`):

```rust
let manifest_path = root.join("manifest.toml");
let toml_source = std::fs::read_to_string(&manifest_path)...;
```

Every *runtime* file read in `DirectorySource` goes through
`resolve_inside_root` → `validate_gadget_path` +
`canonicalize`/`starts_with` (`source.rs:180-221`), which rejects a
symlink that resolves outside the gadget root. The manifest read at
`open()` time honours none of that: if `manifest.toml` (or a
directory component of `root`) is a symlink, `read_to_string`
follows it out of the root.

## Impact
On load, a symlinked `manifest.toml` causes the host to read an
arbitrary external file and parse it as the gadget manifest. Impact
is bounded: the bytes must parse as a valid gadget manifest for the
load to succeed, and the content is consumed into the `Manifest`
struct rather than returned to the guest, so this is not a
general-purpose file-exfiltration primitive. It is a symlink-follow
out of root at a trusted-load boundary, inconsistent with the
guard applied to every other `DirectorySource` read. `DirectorySource`
backs dev gadgets and directory-form gadgets only (user gadgets are
`.torchsnap` archives), and combined with the unvalidated command
`cwd` (a gadget with a file-creating command rule can plant the
symlink at its own root — see
`../host-core/01kwh4j9bptrayf451yzd2145e-command-cwd-unvalidated.md`)
a directory-form gadget could redirect its own manifest read on the
next load. Preliminary severity: low.

## Suggested fix
Apply the same symlink discipline to the manifest read as to file
reads: canonicalize `manifest_path` and verify it is under the
canonicalized root before reading (or open the final component with
`O_NOFOLLOW`), consistent with `resolve_inside_root`. Alternatively,
if the gadget root is considered trusted at `open()` time, document
that assumption explicitly so the inconsistency with the runtime
read guard is a recorded decision rather than an oversight.
