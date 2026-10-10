---
title: "Return an error instead of panicking on SqlValue::List in the ToSql impl"
kind: bug
component: backend
origin: review
tags: [security, error-handling]
---
# Return an error instead of panicking on SqlValue::List in the ToSql impl

Found in the unsafe and fragile patterns audit: a codebase audit for remaining unsafe blocks, fragile pointer patterns, and unrecoverable panics in production code paths. This is one of three items that audit flagged; the other two are filed as separate todos. The audit cited `src-tauri/src/storage/sql_storage.rs:119`; the `panic!` is now around line 120.

## Problem

`SqlValue::List(_)` hits a `panic!` inside `to_sql()`, which is
called by rusqlite during query execution. The caller has no way to
catch this through normal error handling. It crashes the thread.

```rust
SqlValue::List(_) => panic!("List values must be expanded before binding"),
```

## Suggested fix

Return an `Err` instead:
```rust
SqlValue::List(_) => Err(rusqlite::Error::ToSqlConversionFailure(
    "List values must be expanded before binding".into(),
))
```
