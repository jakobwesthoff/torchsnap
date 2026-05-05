# 19. Provide file-based blob storage for plugins via FileStorage

Date: 2026-03-27

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

Plugins need to store binary data — app icons, clipboard images, file
thumbnails, and similar blobs that do not belong in SQLite. The current
`IconCache` handles this for app icons but combines storage, caching,
and invalidation into a single component with no concept of plugin
ownership.

With per-plugin data isolation (ADR 0017, 0018), each plugin needs its
own file-based storage. Binary data should be kept out of SQLite
(ADR 0018) to keep databases lean and queryable. Plugins reference
stored blobs by key in their `SqlStorage`.

## Decision

### FileStorage

A generic file-based storage primitive for binary data. Instantiated
with a string ID that determines its subdirectory under the plugin's
data path. A plugin can have multiple `FileStorage` instances for
different purposes.

````
<app_data_dir>/plugin-home/<plugin-id>/
├── sql/
│   └── storage.sqlite3         # SqlStorage (ADR 0018)
├── icons/                      # FileStorage("icons")
├── thumbnails/                 # FileStorage("thumbnails")
````

The per-plugin state root at `<app_data_dir>/plugin-home/<plugin-id>/`
is formalized in ADR 0035, which splits plugin *code* (under
`<app_data_dir>/plugins/`) from plugin *state* (under
`<app_data_dir>/plugin-home/`).

**API:**

````rust
pub struct EntryMetadata {
    pub modified: SystemTime,
    pub size: u64,
}

fn store(&self, key: &str, data: &[u8]) -> Result<()>;
fn load(&self, key: &str) -> Result<Option<Vec<u8>>>;
fn delete(&self, key: &str) -> Result<()>;
fn exists(&self, key: &str) -> bool;
fn metadata(&self, key: &str) -> Option<EntryMetadata>;
fn entries(&self) -> impl Iterator<Item = (String, EntryMetadata)>;
````

`FileStorage` is dumb storage — it does not interpret file contents or
make invalidation decisions. The `entries()` method returns an iterator
(not a collected list) that yields key + metadata together, supporting
efficient bulk operations like cleanup sweeps without loading all keys
into memory.

### Caching as a separate layer

Caching behavior — mtime-based invalidation, orphan cleanup, in-memory
lookup — is a separate concern layered on top of `FileStorage` by
consumers. The existing `IconCache` will be refactored into a cache
layer that uses `FileStorage("icons")` for persistence and adds
invalidation and in-memory caching on top. `FileStorage` itself has no
cache semantics.

### WASM boundary

Like `SqlStorage`, `FileStorage` operations use serializable types
(keys are strings, data is byte arrays). For WASM plugins, the host
mediates all file I/O through host functions using the same API.

## Consequences

* Plugins get a clean primitive for binary data that is independent of
  their SQL storage.
* Multiple `FileStorage` instances per plugin allow logical separation
  (icons, thumbnails, exports, etc.) without naming conflicts.
* The current `IconCache` is refactored into a cache layer on top of
  `FileStorage`, separating storage from invalidation policy.
* `FileStorage` stays simple — no cache logic, no interpretation of
  contents, no automatic cleanup. Consumers decide policy.
* Plugin data isolation extends to binary data: each plugin's files live
  under its own directory. Removing a plugin removes everything.