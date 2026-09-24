---
kind: bug
severity: low
status: open
area: [src-tauri/src/gadget_install.rs, src-tauri/src/lib.rs, src-tauri/src/gadget_host.rs]
tags: [security, unconfirmed]
---

# Install/uninstall trust chain holds (load re-validates; `remove_dir_all` symlink-safe), but the loader has no duplicate-id gate → builtin-id shadowing

## Problem
Re-examined install and uninstall as a unit. The core trust chain is
sound; the notable finding is a loader robustness gap.

### Install TOCTOU — neutralized by load-time re-validation
`install_impl` (`gadget_install.rs:108-174`) validates the archive via
`ArchiveSource::open(archive_path)`, drops the handle, then
`std::fs::copy(archive_path, …)` re-opens `archive_path`, so a same-user
process could swap the file between validation and copy. This does not
bypass validation: the startup load path re-validates whatever bytes
landed on disk. `load_wasm_gadgets` (`lib.rs:1031`) → `scan_gadget_entries`
(`wasm/discovery.rs:107`) → `open_gadget_source` →
`ArchiveSource::open(final_path)` (`lib.rs:1145`) re-runs the zip parse,
`Manifest::parse` (including `GadgetId` charset validation via its
`Deserialize` impl, `wasm/manifest/mod.rs:174-204`), and the path guard.
The runtime gadget id is taken from the manifest inside the archive
(`lib.rs:1067`), never the filename stem (the stem is used only for
archive-over-directory shadowing, `discovery.rs:123-148`). So no
install-time decision is trusted at load.

Residual consequences of a swap that changes the manifest id (file
`old-id.torchsnap` containing manifest id `new-id`):
- Load registers as `new-id` and works. Uninstall of `new-id` builds
  `gadgets/new-id.torchsnap` (`:220-225`), but the file on disk is
  `old-id.torchsnap`, so `archive.exists()` is false and the removal is
  silently skipped; gadget-home and settings (keyed by manifest id) are
  wiped correctly. The gadget **resurrects on next restart** with fresh
  state — a zombie install, not code-execution or traversal.

### Builtin-id shadowing (the genuine loader gap — reachable without any race)
The install-time collision check (`gadget_install.rs:126-141`) is the
**only** id-collision gate for User archives. `loaded_ids` in
`load_wasm_gadgets` starts empty (`lib.rs:1044`) and `register_with_caps`
pushes without any duplicate check (`gadget_host.rs:373-393`). Builtins
register first. So a swapped — or simply manually dropped — archive
whose manifest id equals a builtin id (e.g. `clipboard-manager`)
registers a **second slot with the same id**. `gadget_sources()`
collapses duplicates via a `HashMap` collect where the later (User)
insert wins (`gadget_host.rs:399-404`), so `uninstall_user_gadget`
passes the `User`-kind gate for what slot iteration otherwise treats as
the builtin, and deletes `gadget-home/<builtin-id>/` — the builtin's
storage. Meanwhile `find`-based routing returns the builtin (first
slot) while queries iterate both slots. Inconsistent, and it launders a
builtin's id through the uninstall gate. This is reachable by dropping
a file into `<app_data_dir>/gadgets/`, no race required.

### Uninstall `remove_dir_all` — safe (verified empirically)
Traversal is impossible: both delete targets are
`app_data_dir.join(...).join(gadget_id)` with the id guaranteed
`[a-z0-9-]` at parse (`manifest/mod.rs:185-204`), so no `/`, `.`, `..`.
Symlink behavior verified by compiling a probe on the local toolchain
(rustc 1.94.0, macOS; no toolchain pin, so current stable ships):
- Top-level entry is a symlink to an outside dir → `Path::exists()`
  follows it (true), then `remove_dir_all` removes **the link itself**;
  the target and its contents survive.
- Symlinked child inside the tree → unlinked as a link, never
  traversed; target survives.
- Dangling top-level symlink → `exists()` false, skipped (stale link
  left behind, cosmetic).
This is the post-CVE-2022-21658 std behavior (fd-based `openat` +
`O_NOFOLLOW`, fixed in Rust 1.58.1); any toolchain this project builds
with has it. The attack premise is otherwise plausible — gadgets have
zero WASI filesystem (no preopens, `wasm/runtime/engine.rs:155-161`)
and `FilesystemCap` is read-only, but a `command` grant defaults its
cwd to `gadget-home/<id>/exec-cwd/` (`caps/command.rs:177-189`), so a
granted binary could plant symlinks inside gadget-home — but it cannot
weaponize them against `remove_dir_all`. `remove_file` on a symlinked
`<id>.torchsnap` likewise unlinks the link, not the target.

### Other trust-chain notes
- Id charset at install is enforced (`install_impl` takes the id from
  `Manifest::parse` → `GadgetId::deserialize`; no unvalidated id reaches
  `final_path`/`tmp_path`).
- Tmp staging file (`.{id}.torchsnap.tmp`): predictable name; `fs::copy`
  follows a pre-planted symlink at `tmp_path`, and two concurrent
  installs of the same id share the tmp path and could interleave into a
  corrupt archive. Both are same-user, and a corrupt result fails load
  re-validation and is skipped with a logged error. Negligible.
- Session-frozen collision check: `host.gadget_sources()` is frozen at
  startup, so installing id X twice before restarting silently
  overwrites the first (last-write-wins) instead of surfacing "already
  installed". UX/consistency footnote.
- Install performs no decompression (zip-bomb deferred, tracked in the
  archive-size todo) and imposes no size cap on the copy — a size sanity
  check at install is a natural anchor for that todo.

## Impact
Severity **low**. Every exploit precondition is same-user, which
already equals full control over app-data (a same-user process can drop
a file into `gadgets/` directly — strictly easier than the install
race). The gadget→host boundary holds: ids are charset-bound, load
re-validates all bytes, `remove_dir_all` is symlink-safe on the shipped
toolchain, and the `User`-kind gate bounds uninstall. The
builtin-id-shadowing gap is a real correctness/robustness issue
(uninstall can wipe a builtin's storage, and routing goes
inconsistent), but not a privilege boundary crossing.

## Suggested fix (priority order)
1. **Seed the loader dedup with registered ids — the one load-bearing
   fix.** In `load_wasm_gadgets`, initialize `loaded_ids` from
   `host.gadget_sources().keys()` (or make `register_with_caps` bail on
   a duplicate id). This closes builtin-id shadowing for *any* file that
   lands in the user gadgets dir, race or no race.
2. **Reorder install to validate-what-you-publish.** Copy `archive_path`
   to a randomized tmp name first, run `ArchiveSource::open` on the
   *tmp copy*, then rename to `<id>.torchsnap` using the id from that
   validated copy. Kills the TOCTOU and the stem/id mismatch at the
   root (the current order cannot, since the tmp name depends on the
   not-yet-known id).
3. **Enforce `file_stem == manifest id` for User-root archives at load**
   (skip + error log on mismatch). Restores the filename↔id bijection
   uninstall depends on and makes manual drops self-consistent.
4. **Uninstall: log (don't silently skip) when the expected archive
   file is absent** — the observable symptom of every mismatch scenario.
5. **Size cap at install** (fold into the archive-size todo), and a
   one-line comment on the `remove_dir_all` calls noting the
   symlink-safety reliance on post-1.58.1 std, so a future refactor to a
   custom recursive delete does not silently regress it.

## Related
- Archive decompressed-size cap (install-time size check anchors here):
  `../host-wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`.
- Uninstall-of-live-gadget state resurrection:
  `01kwh2e8mne5bd05tpb4paacwc-uninstall-live-gadget-resurrects-state.md`.

## Files
`gadget_install.rs:108-174,197-270`; `lib.rs:1031-1147`;
`gadget_host.rs:373-404`; `wasm/manifest/mod.rs:174-204`;
`wasm/discovery.rs:107-153`; `caps/command.rs:177-189`.
