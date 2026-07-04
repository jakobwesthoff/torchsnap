# Clipboard storage multi-statement operations are not atomic — races leave orphan files and dangling file refs

**Kind:** possible-bug
**Severity:** medium
**Area:** src-tauri/src/gadgets/clipboard/storage.rs

## Problem

Every mutating operation in `SharedState` issues multiple
independent SQL statements (plus FileStorage writes/deletes)
without a transaction, while three threads operate on the same
state concurrently: the watcher thread (`store_entry`), the
retention thread (`delete_expired_entries`), and the message
handler (`delete_entry`, `clear_all`, `touch_entry`, `paste`).

Concrete interleavings:

1. **`delete_expired_entries` vs `touch_entry` (paste).**
   `storage.rs:347-368` first SELECTs expired ids, deletes their
   FileStorage files, then re-runs the cutoff WHERE in a separate
   DELETE with a *re-evaluated* `'now'`:
   ```rust
   let old_ids: Vec<String> = ... WHERE captured_at < strftime(..., 'now', ?1)...;
   for id in &old_ids { self.delete_file_storage_for_entry(id); }
   self.sql.execute("DELETE ... WHERE captured_at < strftime(..., 'now', ?1)", ...);
   ```
   If the user pastes an entry between the SELECT and the DELETE,
   `touch_entry` bumps `captured_at` to now: the row survives the
   DELETE, but its files were already deleted. The surviving entry
   permanently references missing files (paste of it then skips
   formats with an stderr log, `storage.rs:217-231`).
   Conversely, an entry crossing the cutoff between the two
   statements is deleted by the re-evaluated WHERE without its
   files ever being collected — orphan files on disk that nothing
   cleans up (FileStorage has no orphan GC here; `clear_all` also
   only deletes *referenced* keys).

2. **`store_entry` partial failure.** `storage.rs:283-326` inserts
   into `clipboard_entries`, then `clipboard_display`, then one
   row per format (some via FileStorage). An error midway (disk
   full during `files.store`, for example) leaves an entry without
   display/content rows. The watcher logs the error and moves on;
   the half-entry stays in history forever ("(empty)" row).

3. **`delete_entry` / `clear_all`** delete files first, SQL rows
   second (`storage.rs:332-344,427-447`); a crash between the two
   leaves rows whose files are gone (recoverable only by manual
   delete).

The existing todo
`todos/01krxrg2xe9jqhb0hgctvbsskd-evaluate-sql-transaction-wrapper.md`
is about the *WIT/gadget-facing* SQL interface; this todo is about
the host-side clipboard gadget using `SqlStorage` directly.

## Impact

Low-probability races, but each one leaves permanent artifacts:
entries that can't be pasted, invisible orphan files accumulating
in the gadget's file storage, or ghost entries. None are cleaned
up later.

## Suggested fix

- Make `delete_expired_entries` operate on the captured id set
  (`DELETE ... WHERE id IN (...)`) instead of re-evaluating the
  cutoff, and delete files *after* the SQL delete succeeds.
- Wrap `store_entry`'s SQL statements in a transaction (BEGIN /
  COMMIT via `SqlStorage`), writing FileStorage content before the
  COMMIT so a failed file write aborts the entry.
- Consider a startup orphan sweep: FileStorage keys not referenced
  by any `clipboard_content.file_key` get deleted (bounded, runs
  once per enable).
