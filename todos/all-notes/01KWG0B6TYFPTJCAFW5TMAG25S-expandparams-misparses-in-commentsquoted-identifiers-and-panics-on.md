---
title: "expand_params misparses ? in comments/quoted identifiers and panics on mismatch"
kind: bug
component: backend
impact: low
---
# expand_params misparses ? in comments/quoted identifiers and panics on mismatch

`expand_params` in `src-tauri/src/storage/sql_storage.rs` walks the SQL text to substitute list parameters. It skips single-quoted literals only, so a `?` inside a comment or a double-quoted identifier desynchronizes placeholders from parameters, and a mismatch panics while the connection mutex is held. Only host-internal callers can reach it today, since the WIT `sql-value` has no list variant. Read in the code, not reproduced.

## Problem
`expand_params` walks the SQL string to substitute
`SqlValue::List` placeholders
(`src-tauri/src/storage/sql_storage.rs:428-488`). It skips
single-quoted string literals but nothing else. A `?` occurring
inside a double-quoted identifier (`"weird?col"`), a line
comment (`-- what?`), or a block comment (`/* ? */`) is treated
as a placeholder, desynchronizing the SQL-to-params pairing for
every subsequent placeholder. (Doubled `''` escapes inside
literals happen to work because the parser re-enters literal
mode on the second quote.)

When the pairing desynchronizes, or when the caller simply
passes fewer params than placeholders, the walker panics:

```rust
let param = param_iter.next().expect("more ? placeholders than params");
```

(`sql_storage.rs:457`). The expansion path only runs when a `List` param is
present, and the WIT `sql-value` has no list variant, so guests
cannot reach this today; only host-internal callers can. Still,
a host-side query with a comment containing `?` plus a `List`
param is an easy future trap, and the failure is a panic while
holding the connection mutex, which poisons it. See the todo on
mutex poisoning under Relations.

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

## Relations

- Relates to: [Mutex-poisoning `expect`s turn one panic into a permanent cascade](01KWFZ4KKAQ7SPWNM2NCKET1GE-mutex-poisoning-expects-turn-one-panic-into-a-permanent-cascade.md), the same connection mutex that this panic poisons.
