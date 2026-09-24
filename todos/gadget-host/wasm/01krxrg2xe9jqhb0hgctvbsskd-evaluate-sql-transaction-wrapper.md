---
kind: question
status: open
---

# Evaluate SQL transaction wrapper in WIT interface

Gadgets currently manage SQLite transactions manually via
`db.execute("BEGIN", &[])` / `COMMIT` / `ROLLBACK`. Evaluate whether the
`sql-storage` WIT interface should provide a dedicated transaction mechanism.

## Options to consider

- A `transaction` resource with automatic rollback on drop/error.
- A `with_transaction(callback)` style wrapper (if expressible in WIT).
- Keeping it manual but adding SDK-level helpers in the Rust crate.

## Questions

- Is there a WIT-idiomatic way to express scoped resources with cleanup
  semantics?
- Would a transaction wrapper add meaningful safety, or is the manual
  approach sufficient given single-threaded WASM execution?
- Are there gadgets where transaction misuse has caused issues?
