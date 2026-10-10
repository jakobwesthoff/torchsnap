---
title: "Make calculator history query failures visible and clarify the SQL config error"
kind: improvement
component: gadgets
impact: low
origin: review
tags: [error-handling, logging]
---
# Make calculator history query failures visible and clarify the SQL config error

Two polish items surfaced by the post-migration validation review of 2026-04-08 (seven parallel review agents covering D1-D6 and the calculator port). The critical and important findings of that review were fixed in 11 follow-up commits. These items are lower-priority polish that was deliberately deferred. Neither blocks current functionality. They land together as one small Rust PR. Each item keeps the original review section number so back-references stay intact.

## #28: Clearer `SqlConfig::None` error message

**Where:** `src-tauri/src/wasm/runtime.rs`, in `bindings::torchsnap::gadget::sql::Host::open` (around the `SqlConfig::None` arm). `runtime.rs` no longer exists under `src-tauri/src/wasm/` (checked 2026-10-10); search for `SqlConfig::None` to find the arm's current home.

**Current error:** `gadget has no [storage.sql] declared in manifest.toml`

**The thing:** accurate, but it misses the case where the gadget has a `[storage]` table and forgot the `[storage.sql]` sub-table, or has `[storage.sql]` with an empty `migrations` key.

**Fix:** rephrase so the message names the action:

```
gadget has no [storage.sql] block declared in its manifest.toml — add a [storage.sql] migrations = [...] entry to enable SQL storage
```

A trivial string change. The message is host code (`gadget-host`), so this item touches `src-tauri` while #29 touches the calculator gadget.

## #29 and #33: Visibility for `query_history` errors in calculator

**Where:** `gadgets/calculator/src/lib.rs`, the `query_history` function.

**Current behaviour:** `db.query(...)` errors are silently swallowed (`Err(_) => return vec![]`), which produces an empty history list with no trace in the dev tools. Still the case on 2026-10-10.

**Fix:** replace the swallow with a `logging::log` call so query failures appear in the host's devtools console:

```rust
let rows = match db.query(&sql_text, &params) {
    Ok(r) => r,
    Err(e) => {
        logging::log(
            logging::LogLevel::Warn,
            &format!("calculator history query failed: {e}"),
            &[],
            None,
        );
        return vec![];
    }
};
```

The fallback to an empty list is correct (search should not crash on a failed history query). The failure just has to be visible.

The same defensive logging should be added where `expect_text`'s filter_map drops a row with a non-Text column, though that path is currently unreachable given the schema.

## Related review item tracked elsewhere

A dedicated `gadgets/test-fixture/` crate for end-to-end WASM bridge integration tests against the wasmtime linker. The migration plan required it for the D1-D4 layered tests, but it was never created. It would unblock the SQL storage tests the calculator port had to skip (`history_save_and_query`, `history_dedup_bumps_timestamp`, `history_filter`), which would also cover the `query_history` path above.
