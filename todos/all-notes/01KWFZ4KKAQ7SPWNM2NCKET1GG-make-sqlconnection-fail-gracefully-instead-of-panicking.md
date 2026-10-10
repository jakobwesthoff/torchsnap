---
title: "Make sql::connection() fail gracefully instead of panicking"
kind: bug
component: gadget-host
---
# Make sql::connection() fail gracefully instead of panicking

The `sql::connection()` host import has two `expect` calls that panic inside the host. A gadget that leaks SQL handles can reach the second one. The first is nominally unreachable, but its message is written as guidance for gadget authors, so it reads as a path the host expects to hit.

## Problem
Read in the code, not reproduced.

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
   host import while the store mutex is held. A panic in that
   state poisons the mutex (see the Relations section).
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
is reportable to the guest.

## Relations

- Relates to: [Mutex-poisoning `expect`s turn one panic into a permanent cascade](01KWFZ4KKAQ7SPWNM2NCKET1GE-mutex-poisoning-expects-turn-one-panic-into-a-permanent-cascade.md), a panic inside a host import poisons the store mutex
- Relates to: [Replace `thread_local!` gadget state with WIT resource exports](01KR22RJKE30JCAHQYJ20Y7DQJ-replace-threadlocal-gadget-state-with-wit-resource-exports.md), the WIT resource lifecycle of the same handles
