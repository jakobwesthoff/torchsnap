# Add an in-memory constructor to SqlStorage for tests

Status: open — investigated during the interface-gate flake fix
(commit `db910a8`), design sketched below, not yet decided.

Scope: `src-tauri/src/storage/sql_storage.rs`, plus optional migration
of test helpers across the crate. This is a test-ergonomics and
test-speed change; no production behaviour changes.

## Background

`SqlStorage` exposes exactly one constructor
(`sql_storage.rs:283`):

```rust
pub fn open(db_path: PathBuf, migrations: &[&str]) -> Result<Self>
```

Every test that needs storage must therefore create a real file. The
established pattern is a `tempfile::tempdir()` whose `TempDir` the test
keeps alive, as in `frecency/mod.rs:463` and
`gadgets/clipboard/storage.rs`. There are 21 `SqlStorage::open` call
sites in `src-tauri/src`, the majority in test modules.

This surfaced while fixing a flaky test. `wasm/interface_gate.rs`
`full_caps()` skipped the tempdir pattern and hardcoded one shared path
in the system temp dir, so five parallel tests raced to create the same
file and intermittently failed with "database is locked". Fixed in
`db910a8` by returning a `TempDir` from `full_caps()`.

The flake is resolved, so this todo is not a bug fix. The point is that
the API made the wrong thing the easy thing: a caller who wants a
throwaway database has to think about file paths, directory lifetime,
and cross-test collisions, none of which the test actually cares about.

## Why an in-memory constructor is the right shape

For a test that only needs a schema and a few rows, SQLite's
`:memory:` database is the natural fit: no filesystem, no cleanup, no
possibility of two tests colliding on a path, and faster.

Verified against the SQLite this project actually ships (rusqlite 0.40
with `bundled`, `src-tauri/Cargo.toml:42`, SQLite 3.53.2), by probing
through the real code rather than the system `sqlite3` CLI, which is a
different build:

- `configure_connection` (`sql_storage.rs:516`) runs **unchanged**
  against an in-memory connection and returns `Ok(())`. No branch is
  needed for the pragma block.
- `journal_mode = wal` is not an error on an in-memory database; it
  resolves to `memory`. This matters because a naive expectation would
  be that WAL is rejected and the constructor needs a separate pragma
  path. It does not.
- `foreign_keys = on` and `busy_timeout` both apply normally. Foreign
  keys matter: the clipboard schema relies on `ON DELETE CASCADE` for
  FTS index cleanup, so a test database that silently lost FK
  enforcement would be worse than useless.

## Proposed design

```rust
/// Open an in-memory database and run `migrations` against it.
///
/// The database lives only as long as the returned `SqlStorage` and is
/// never written to disk. Intended for tests.
pub fn open_in_memory(migrations: &[&str]) -> Result<Self>
```

Implementation is `open()` with `Connection::open_in_memory()`
substituted for `Connection::open(&db_path)` and the `create_dir_all`
step skipped; pragmas and the `rusqlite_migration` run are identical.
Factor the shared tail into a private helper rather than duplicating
it, so the two constructors cannot drift on pragma or migration
handling.

Open questions to settle before implementing:

1. **Should it be `#[cfg(test)]`?** Arguments both ways. Gating it to
   test builds keeps the production API surface minimal and makes the
   intent unambiguous. Leaving it public costs nothing at runtime and
   would let gadget-host code use an ephemeral database if a use case
   ever appears. Recommendation: `#[cfg(any(test, feature = "..."))]`
   is over-engineering for now — start with plain `pub` and a doc
   comment saying it is for tests, or `#[cfg(test)]` if the API
   guidelines argument wins. Decide, do not leave it accidental.
2. **Does each in-memory connection need to be distinct?** Plain
   `:memory:` gives every connection its own private database, which is
   what tests want. Shared-cache URIs (`file:name?mode=memory&cache=shared`)
   do the opposite and would reintroduce exactly the cross-test
   collision this is meant to eliminate. Do not use them without a
   concrete reason.
3. **`SqlStorage` holds `conn: Mutex<Connection>`.** Confirm nothing in
   the type assumes a persistent path (it does not appear to — there is
   no stored `db_path` field), so no struct change is needed.

## Migration of existing tests

Optional and incremental; the constructor is useful even if nothing is
migrated on day one.

Candidates, in rough order of benefit:

- `wasm/interface_gate.rs` `full_caps()` — the motivating case. Would
  drop the `TempDir` from the return type entirely and revert the
  signature back to `fn full_caps() -> ProvisionedCaps`, removing the
  `let (caps, _dir) = ...` noise from five call sites.
- `caps/sql_storage.rs` tests — three call sites, all throwaway
  schemas.
- `storage/sql_storage.rs` own tests — eight call sites.
- `frecency/mod.rs`, `network/website_metadata/cache.rs`,
  `wasm/bridge.rs` — check each; some may deliberately test on-disk
  behaviour.

**Do not migrate blindly.** Any test that asserts something about
persistence, file layout, WAL files, or reopening a database must keep
using a real file. Grep for tests that call `open()` twice on the same
path before converting anything.

`gadgets/clipboard/storage.rs` is a deliberate keep-on-disk case: its
18 integration tests exercise FTS5 triggers and `ON DELETE CASCADE`
against the production schema, and while those would work in memory,
the suite is also the closest thing to a regression harness for real
database behaviour. Converting it buys little and removes fidelity.

## Test coverage

For the constructor itself, in `storage/sql_storage.rs` tests:

- Migrations run: a table created by a migration exists afterwards.
- Multiple migrations apply in order and each runs once.
- `foreign_keys` is actually on — insert a child row referencing a
  missing parent and assert it fails. This is the pragma most likely to
  regress silently and the one the clipboard schema depends on.
- Two `open_in_memory` calls produce genuinely independent databases:
  write to one, assert the other does not see it. This is the guard
  against someone later "optimising" it into a shared-cache URI.
- The storage is usable through the normal API surface: `execute`,
  `query_map`, and `transaction` all work, including a rollback.
- A savepoint/nested transaction case, since `savepoint_counter` is
  per-instance state.

For any migrated test: no new assertions needed, but run the affected
suites under `--test-threads=16` a few times, since the whole point is
concurrency safety. The interface-gate flake needed 16 threads *and* a
missing file to show up at 5 failures in 15 runs; a single green run
proves nothing.

## Documentation

- `CHANGELOG.md` entry.
- Rustdoc on the new constructor stating it is for tests and that the
  database is per-connection and non-persistent.
- If the tempdir-vs-memory choice is worth guidance for future tests,
  a short note in the module header of `storage/sql_storage.rs` is
  enough. No ADR: this is an additive test helper, not an
  architectural commitment.
