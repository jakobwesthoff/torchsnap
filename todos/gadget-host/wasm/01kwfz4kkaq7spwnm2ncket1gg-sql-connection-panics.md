# sql::connection() panics instead of failing gracefully

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/wasm/runtime/host/sql.rs

## Problem
The `sql::connection()` host import has two panic paths
(`src-tauri/src/wasm/runtime/host/sql.rs:43-60`):

```rust
fn connection(&mut self) -> Resource<SqlHandleEntry> {
    let sql_cap = self.caps.sql_storage.as_ref().expect(
        "sql::connection() called but no SQL storage is initialized — \
         declare [storage.sql] in manifest.toml",
    );
    ...
    let handle = self.wasi_table.push(entry)
        .expect("allocate SQL handle in resource table");
```

1. The first `expect` is nominally unreachable because the
   interface gate refuses to load a gadget importing
   `sql-storage` without a provisioned cap, but the expect
   message is written as author-facing guidance, i.e. it is
   treated as reachable. If it ever fires it panics inside a
   host import while the store mutex is held (see the
   mutex-poisoning todo
   `01kwfz4kkaq7spwnm2ncket1ge-mutex-poisoning-cascade.md`).
2. `wasi_table.push` returns an error when the resource table
   is exhausted. A gadget that calls `connection()` in a loop
   without dropping handles grows `sql_handle_reps` unboundedly
   and eventually panics the host thread via this `expect`. The
   guest fully controls how often `connection()` is called, so
   host stability depends on guest good behavior.

The WIT signature (`connection: func() -> sql-handle`) offers no
error arm, which is why the code panics rather than returns.

## Impact
A buggy or hostile gadget can panic a host thread (and poison
the store mutex, wedging the gadget permanently) just by leaking
SQL handles.

## Suggested fix
Short term: replace both `expect`s with
`Err(wasmtime::Error)`-style traps (host import functions can
return `wasmtime::Result<Resource<_>>`, which traps the guest
instead of panicking the host), and cap the number of live
handles per gadget with a small limit since they all share the
same `Arc<SqlStorage>` anyway. Longer term: consider giving
`connection()` a `result` return in the WIT so misconfiguration
is reportable to the guest. Related WIT-resource lifecycle todo:
`todos/gadget-host/wasm/01kr22rjke30jcahqyj20y7dqj-wit-resource-instance-state.md`.
