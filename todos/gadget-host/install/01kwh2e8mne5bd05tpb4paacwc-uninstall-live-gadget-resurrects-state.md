---
kind: bug
severity: medium
status: open
area: [src-tauri/src/gadget_install.rs]
plan: todos/plans/01m399736658afgk6xv4ab68wk-open-gadget-archives-from-outside-the-app.md
---

# Uninstalling a running gadget deletes state under a live instance, which can resurrect it

## Problem

`uninstall_impl` (`src-tauri/src/gadget_install.rs:197-270`)
deletes the gadget's on-disk state while the gadget is still
loaded and running. The flow returns
`requires_restart: true` and the module comment relies on the
restart happening ("the gadget can never reach it after the
restart the caller is about to perform",
`gadget_install.rs:235-237`), but nothing forces or performs
that restart — the user can dismiss the banner and keep using
the app.

Between uninstall and the (optional) restart, the gadget's WASM
instance, its open SQLite connection, its registered global
shortcuts, and its scheduled cron tasks all remain active:

- `remove_dir_all` on `<app_data_dir>/gadget-home/<id>/`
  (`gadget_install.rs:238-241`) unlinks the gadget's
  `storage.sqlite3` plus its `-wal`/`-shm` siblings under an
  open `rusqlite` connection. On macOS/Linux the unlink
  succeeds but the connection keeps writing to the unlinked
  inode; any subsequent operation that makes SQLite re-create
  the WAL/SHM files (checkpoint, reconnect-on-error) writes new
  files into a re-created directory path. Either way the
  "cleaned" state is partially resurrected or silently diverges
  from what the user believes was deleted.
- The settings cleanup (`gadget_install.rs:251-265`) strips
  `enabled.<id>` and `gadgets.<id>.*` keys, but the still-live
  gadget (or its settings UI panel, still mounted until
  restart) can write settings again through the normal settings
  capability, re-creating the keys that were just stripped.
- Cron tasks keep firing and searches keep routing to the
  gadget until restart, so the gadget can re-populate anything
  it persists.

The existing todo
`todos/gadget-host/wasm/01kpdsvj5at6agxst1jva5eeva-gadget-hot-lifecycle.md`
tracks the long-term fix (a real teardown path: disable →
drop instance → close SQL connection → stop cron → then
delete). This todo tracks the interim correctness gap in the
shipped restart-based flow, which that plan explicitly treats
as the "known-good fallback" — currently it is not actually
good: cleanup runs at the wrong time relative to instance
teardown.

## Impact

Uninstalling a gadget while continuing to use the app can leave
behind (or re-create) `gadget-home/<id>/` contents and
`gadgets.<id>.*` settings keys — precisely the state the
uninstall cleanup exists to remove. No error is surfaced; the
user sees a successful uninstall.

## Secondary observation: deletions bypass the settings-changed pipeline

The key deletions also never emit the `settings-changed` Tauri
event. That event is the sole trigger for the whole settings
reactivity pipeline — Rust-side `SettingsNotifier`, gadget
`setting_changed` dispatch, shortcut re-registration
(`src-tauri/src/lib.rs:885-913`) and every webview's
`settingsStore` cache (`src/settingsStore.ts:87-91`). Verified
2026-07-02: the only emitter of `settings-changed` in the repo
is the frontend `setSetting` (`settingsStore.ts:145`); Rust's
other two store writes are startup-only
(`gadget_host.rs:429-432`, `settings/mod.rs:111`), so uninstall
is the one post-startup mutation that leaves all subscribers
unnotified. Today this is masked by the required restart, but
any fix for the main finding (or a future hot-uninstall) must
route the deletions through the notification pipeline.

## Secondary observation: settings save error swallowed

`uninstall_impl` ends with `let _ = store.save();`
(`gadget_install.rs:265`). If persisting the settings store
fails, the key deletions live only in memory and reappear from
disk on the next launch, again leaving stale
`enabled.<id>`/`gadgets.<id>.*` keys — silently, while the
command still reports success. Every other failure in this flow
propagates via `.context(...)?`.

## Suggested fix

Two options, not mutually exclusive:

1. Minimal interim fix: before deleting files, disable the
   gadget in the host (stop routing searches/executes to it)
   and defer the `gadget-home` + settings cleanup to the next
   startup (e.g. write a `<app_data_dir>/gadgets/<id>.uninstall`
   marker that startup processes before instantiating gadgets).
   Startup-time cleanup runs with no live instance, closing the
   race entirely.
2. Full fix: implement the teardown ordering from the
   hot-lifecycle todo (close SQL connection, stop cron,
   unregister shortcuts, drop instance, then delete).

For the secondary observation: propagate the `store.save()`
error with `.context("persist settings store after uninstall cleanup")?`
instead of discarding it.
