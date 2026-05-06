# Idle instance eviction when launcher is hidden

## Problem

All enabled gadgets hold live `WasmGadgetInstance`s (Store + linear memory +
resource tables) permanently, even when the launcher is hidden and no queries
are running. After the Winch + compile cache changes, idle RSS is ~52 MB. The
Store overhead is a smaller contributor now, but dropping instances when idle
would reclaim it entirely.

## Current state

`CachedComponent.release()` already exists and is called on `disable()`. The
compiled component is dropped from memory while the disk cache is retained.
What's missing is instance-level eviction: dropping the `Store` + linear memory
when the launcher is hidden, without fully disabling the gadget.

The bridge already calls `release()` on the compiled component during
`disable()`. The instance eviction todo is about a lighter-weight operation:
drop the Store but keep the gadget logically enabled (settings, frecency, SQL
config, scheduler state all survive on the bridge).

## Goal

Drop WASM instances when the launcher is dismissed and re-instantiate on demand
when the launcher is shown. Re-instantiation is cheap (~1–5 ms per gadget)
since `CachedComponent.acquire()` re-deserializes from the cache file and
`WasmRuntime::instantiate()` is just linker + Store construction.

## Design

### API

Add to `WasmGadgetBridge` (not the `Gadget` trait — this is WASM-specific):

```rust
fn evict_instance(&self);
```

Distinct from `disable()`: `disable()` tears down the full gadget lifecycle
(clears all capability state, stops scheduler, calls guest `disable()` export,
releases compiled component). `evict_instance()` only drops the `Store` for
memory reclamation. On next `search()` / `entries()` / `execute()`,
`ensure_instance()` re-creates it with the same capability state.

### Re-enable path

`ensure_instance()` already creates a fresh instance when the slot is empty.
The challenge is re-applying all capability state that `enable()` stashes onto
`GadgetState`:
- `set_settings` / `set_frecency` / `set_gadget_source` / `set_path_context`
- `set_sql_storage` (re-open or re-attach the SQLite handle)
- `set_clipboard_writer` / `set_opener_writer` / etc.
- `set_fs_allowlist` / `set_http_client` / `set_command_rules`
- `set_website_metadata`

Most handles are retained on bridge fields. The key question is whether
`enable()` can be factored into "full enable" (first time, runs guest
`enable()` export) vs "warm re-instantiate" (re-apply capability state, skip
guest `enable()` since the gadget was never logically disabled).

### Scheduler interaction

Gadgets with `[[tasks]]` in their manifest must keep their instance alive
regardless of launcher visibility, because the scheduler needs to call
`run_task`. The eviction must skip gadgets with active schedulers.

### Where to hook the launcher lifecycle

- **Dismiss**: `hide_launcher()` in `lib.rs` → call into `GadgetHost` to
  evict idle instances.
- **Show**: `toggle_launcher_window` / global shortcut handler → re-instantiate
  before the frontend sends the first query.

## Key files

- `src-tauri/src/wasm/bridge.rs` — add `evict_instance()`, factor `enable()`
  capability-stashing into a reusable path.
- `src-tauri/src/gadget_host.rs` — add method to evict/warm all WASM instances.
- `src-tauri/src/lib.rs` — hook eviction/warm-up into launcher lifecycle.

## Tests

- `evict_instance()` drops the Store (strong count goes to 0).
- `ensure_instance()` after eviction produces a working instance with all
  capabilities re-applied.
- Gadgets with active schedulers are not evicted.
- Search after eviction + re-instantiation returns correct results.
- Benchmark re-instantiation latency per gadget.

## Priority

Low — idle RSS is already ~52 MB after the compile cache. This would reclaim
the remaining per-instance Store overhead (~5–7 MB across all gadgets), trading
a small latency bump on launcher show for lower idle memory.
