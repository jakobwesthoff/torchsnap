---
kind: bug
severity: low
status: open
area: [src-tauri/src/storage/sql_storage.rs]
tags: [unconfirmed]
---

# expand_params misparses `?` in comments/quoted identifiers and panics on mismatch

## Problem
`expand_params` walks the SQL string to substitute
`SqlValue::List` placeholders
(`src-tauri/src/storage/sql_storage.rs:428-488`). It skips
single-quoted string literals but nothing else. A `?` occurring
inside a double-quoted identifier (`"weird?col"`), a line
comment (`-- what?`), or a block comment (`/* ? */`) is treated
as a placeholder, desynchronizing the SQL↔params pairing for
every subsequent placeholder. (Doubled `''` escapes inside
literals happen to work because the parser re-enters literal
mode on the second quote.)

When the pairing desynchronizes, or when the caller simply
passes fewer params than placeholders, the walker panics:

```rust
let param = param_iter.next().expect("more ? placeholders than params");
```

(`:457`). The expansion path only runs when a `List` param is
present, and the WIT `sql-value` has no list variant, so guests
cannot reach this today; only host-internal callers can. Still,
a host-side query with a comment containing `?` plus a `List`
param is an easy future trap, and the failure is a panic while
holding the connection mutex (poisoning it — see the
mutex-poisoning todo in host-wasm/).

## Impact
Host-internal only for now. Wrong parameter binding or a
panic+poisoned SQL connection for queries combining `List`
params with comments/quoted identifiers containing `?`.

## Suggested fix
Extend the skip logic to `--`/`/* */` comments and
double-quoted identifiers, and replace the `expect` with a
returned error. Alternatively document loudly on
`SqlValue::List` that expansion assumes plain single-quoted-only
SQL. A regression test with `-- huh?` in the SQL pins it either
way.
