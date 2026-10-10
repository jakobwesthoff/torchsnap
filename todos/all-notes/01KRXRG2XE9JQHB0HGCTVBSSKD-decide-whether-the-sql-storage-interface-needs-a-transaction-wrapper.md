---
title: "Decide whether the sql-storage interface needs a transaction wrapper"
kind: investigation
component: gadget-sdk
tags: [api-design]
---
# Decide whether the sql-storage interface needs a transaction wrapper

Gadgets manage SQLite transactions by hand with `db.execute("BEGIN", &[])`, `COMMIT` and `ROLLBACK`. This investigation evaluates whether the `sql-storage` WIT interface should provide a dedicated transaction mechanism, and which option, if any, to adopt.

## Options

- A `transaction` resource with automatic rollback on drop or error.
- A `with_transaction(callback)` style wrapper, if that is expressible in WIT.
- Keeping transactions manual, but adding SDK-level helpers in the Rust crate.

## Open questions

- Is there a WIT-idiomatic way to express scoped resources with cleanup semantics?
- Would a transaction wrapper add meaningful safety, or is the manual approach sufficient given single-threaded WASM execution?
- Are there gadgets where transaction misuse has caused issues?
