---
kind: bug
severity: low
status: open
area: [src-tauri/src/lib.rs, src-tauri/src/gadget_host.rs, src-tauri/src/wasm/discovery.rs, src-tauri/src/gadget_install/registered.rs]
tags: [security, unconfirmed]
---

# The loader has no duplicate-id gate across roots: an archive can shadow a built-in id

## Problem

The install pipeline (ADR 0051) rejects ids of built-in, bundled and
dev gadgets, installs only from a staged copy, and the startup load
re-validates every archive (`ArchiveSource::open` runs the zip parse,
`Manifest::parse` with the `GadgetId` charset check, and the path
guard). The loader itself does not repeat the id check, so a file that
lands in `<app_data_dir>/gadgets/` without going through the install
pipeline is not held to it. Placing a file there by hand is enough; no
race is needed.

### Built-in id shadowing

`loaded_ids` in `load_wasm_gadgets` (`lib.rs:1214`) starts empty, so
it only deduplicates WASM gadgets among themselves. Built-in gadgets
register first, and `register_with_caps` (`gadget_host.rs:368`) pushes
a slot without a duplicate check. An archive whose manifest id equals a
built-in id (for example `clipboard-manager`) therefore registers a
second slot with that id.

`gadget_sources()` (`gadget_host.rs:394`) collects slots into a
`HashMap`, where the later user slot wins. `RegisteredGadgets::from_host`
is built from that map, so the install and uninstall decisions see the
id as a user gadget: uninstall is allowed and the next startup deletes
`gadget-home/<built-in-id>/` and the settings keys of the built-in.
Meanwhile `find`-based routing returns the built-in (first slot) while
queries iterate both slots.

### File name and manifest id can differ

The runtime id comes from the manifest inside the archive; the file
stem is only used for archive-over-directory shadowing
(`wasm/discovery.rs`). A hand-placed `old-id.torchsnap` containing id
`new-id` loads as `new-id`. Uninstalling `new-id` looks for
`gadgets/new-id.torchsnap`, finds nothing to move aside (the uninstall
logs this), and the gadget comes back on the next start with fresh
data.

### Checked and sound

- `remove_dir_all` on `gadget-home/<id>/` removes a symlink itself
  rather than following it (std behaviour since Rust 1.58.1), verified
  with a probe; ids are `[a-z0-9-]` so no traversal. The startup
  cleanup documents this (`gadget_install/archive_ops.rs:190-192`).
- Gadgets have no WASI filesystem and `FilesystemCap` is read-only. A
  granted command runs in `gadget-home/<id>/exec-cwd/` and could plant
  symlinks there, which `remove_dir_all` does not follow.

## Impact

Severity low. Every precondition is same-user write access to the app
data directory, which already allows far more. It is a correctness
and robustness gap: uninstall can wipe a built-in's storage and routing
becomes inconsistent.

## Suggested fix

1. Seed `loaded_ids` in `load_wasm_gadgets` with the ids already
   registered on the host (or make `register_with_caps` reject a
   duplicate id), and log the skipped archive.
2. For the user root, skip an archive whose file stem differs from its
   manifest id, with an error log. This restores the file-name to id
   mapping that uninstall relies on.
3. Add the symlink-safety note to the `remove_dir_all` call in
   `archive_ops::remove_user_gadget` (`archive_ops.rs:129`) as well.

## Related

- Decompressed-size limits for archive entries:
  `todos/gadget-host/wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`.
