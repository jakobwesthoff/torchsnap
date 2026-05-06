# Idle instance eviction when launcher is hidden

## Problem

All enabled gadgets hold live `WasmGadgetInstance`s (Store + linear memory +
resource tables) permanently, even when the launcher is hidden and no queries
are running. The benchmark shows the `torchsnap` process sitting at 242 MB at
idle with the launcher hidden. The memory drops to ~142 MB after extended idle,
suggesting some instances are already being released, but the baseline is still
3× higher than pre-WASM.

## Goal

Drop WASM instances when the launcher is dismissed and re-instantiate on demand
when the launcher is shown. Compilation is cached (especially after the
pre-compiled cache todo), so `instantiate()` is cheap (~milliseconds). This
trades a small latency bump on launcher show for a large reduction in idle
memory.

## Design

### Lifecycle

1. **Launcher shown** → `GadgetHost` (or a new coordinator) calls
   `ensure_instance()` on every enabled WASM gadget. This is already the
   existing code path — `ensure_instance` is idempotent.
2. **Launcher dismissed** → after the dismiss animation completes (the
   `launcher_hide` command in `lib.rs:363`), call a new method on each enabled
   WASM gadget to drop the instance. The compiled `Component` stays cached in
   `WasmRuntime.components` — only the `Store` + linear memory is freed.
3. **Scheduled tasks** — gadgets with `[[tasks]]` in their manifest must keep
   their instance alive regardless of launcher visibility, because the scheduler
   needs to call `run_task` on them. The eviction must skip gadgets that have an
   active scheduler.

### API

Add to the `Gadget` trait (or as a WASM-specific method on `WasmGadgetBridge`):

```rust
fn evict_instance(&self);
```

This is distinct from `disable()`: `disable()` tears down the gadget's full
lifecycle (clears settings, frecency, SQL, scheduler, etc.) while
`evict_instance()` only drops the `Store` for memory reclamation. On next
`search()` / `entries()` / `execute()`, `ensure_instance()` re-creates it with
the same capability state (settings, frecency, SQL config, fs allowlist, etc.)
that was stashed on `WasmGadgetBridge` at construction and `enable()` time.

The re-enable path must re-apply all the capability state that `enable()`
stashes onto `GadgetState`:
- `set_settings` / `set_frecency` / `set_gadget_source` / `set_path_context`
- `set_sql_storage` (re-open or re-attach the SQLite handle)
- `set_clipboard_writer` / `set_opener_writer` / etc.
- `set_fs_allowlist` / `set_http_client` / `set_command_rules`
- `set_website_metadata`

Most of these handles are already retained on `WasmGadgetBridge` fields (the
bridge was designed to survive disable/re-enable). The key question is whether
`enable()` can be factored into "full enable" (first time, runs guest
`enable()` export) vs "warm re-instantiate" (re-apply capability state, skip
guest `enable()` since the gadget was never logically disabled).

### Where to hook the launcher lifecycle

`hide_launcher()` in `lib.rs:363` is the dismiss entry point. After the
window hide, call into `GadgetHost` to evict idle instances. The
`GadgetHost::initialize_and_start()` path (gadget_host.rs:255–271) already
runs `enable()` concurrently via `spawn_blocking` — the show path can use the
same pattern for re-instantiation.

The show path already exists: `toggle_launcher_window` / the global shortcut
handler shows the window. Hook the re-instantiation there, before the frontend
sends the first query.

### Latency budget

Re-instantiation (without Cranelift, just linker + Store + WIT binding) is on
the order of 1–5 ms per gadget. With 7 gadgets done concurrently, the total
wall time is ~5 ms — well within the launcher appear animation budget.
Benchmark this to confirm.

## Key files

- `src-tauri/src/wasm/bridge.rs` — `WasmGadgetBridge`: add `evict_instance()`
  method, factor `enable()` capability-stashing into reusable path.
  `ensure_instance()` (line 331) is the re-instantiation entry point.
- `src-tauri/src/gadget_host.rs` — `GadgetHost`: add method to evict/warm all
  WASM instances; expose to the launcher lifecycle.
- `src-tauri/src/lib.rs` — `hide_launcher()` (line 363) and the show path: add
  eviction/warm-up calls.
- `src-tauri/src/wasm/bridge.rs` — `parsed_tasks` (line 139) and
  `scheduler_handle` (line 144): check these to skip eviction for gadgets with
  active schedulers.

## Tests

- Unit test: `evict_instance()` drops the Store (strong count goes to 0).
- Unit test: `ensure_instance()` after eviction produces a working instance
  with all capabilities re-applied.
- Unit test: gadgets with active schedulers are not evicted.
- Integration test: search after eviction+re-instantiation returns correct
  results.
- Benchmark: measure re-instantiation latency per gadget and total wall time
  for concurrent re-instantiation of all gadgets.
- Benchmark: measure idle RSS with eviction enabled vs disabled.

## Sequencing

Depends on a clear understanding of which capability state survives on the
bridge vs needs re-creation. The pre-compiled cache todo (01kqz0frska2k4...)
should land first — it makes `instantiate()` even cheaper because the
`Component` clone from cache is file-backed rather than heap-backed, reducing
the transient memory spike during re-instantiation.
