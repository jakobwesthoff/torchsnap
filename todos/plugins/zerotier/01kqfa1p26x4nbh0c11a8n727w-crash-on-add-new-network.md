# ZeroTier plugin crashes the host when adding a new network

**Status: open, investigation needed.**

## Repro

Type a 16-hex-char ZeroTier network ID into the launcher that the
plugin has never seen before. The plugin emits the synthetic
"Connect to network `<id>`" entry. Activating that entry crashes
the application.

(If the user instead meant the settings panel "add" button —
which doesn't exist as such, but a manual-token paste followed by
re-render is similar enough to be worth probing — capture which
exact UI surface produced the crash before debugging.)

## What we know

- This path didn't exist before the plugin landed; the crash is
  load-bearing on the launcher's primary "join" workflow.
- Plugin manifest now parses cleanly through the runtime — the
  recent `${...}`-substitution fix unblocked load-time issues, so
  we are far enough into the flow that `enable()` ran, the
  bridge stashed all permission allowlists, and `search()` is
  emitting the synthetic entry. The crash is downstream.
- Plugin Rust code is unwrap-free in production paths
  (`grep -rn "unwrap()" plugins/zerotier/src/` shows test-only
  hits).
- The host should never be killable by a guest panic — wasmtime
  traps panics and the bridge is supposed to surface them as
  `execute()` errors. If the host actually crashes (SIGSEGV /
  Rust panic) versus the plugin returning an error, the bug is
  on the host side, not the plugin.

## Suspected code paths

The synthetic-Connect entry is built in `lib.rs::synthetic_connect_entry`
(`plugins/zerotier/src/lib.rs`) and activates `ActionId::Open`.
Walk these in order when reproducing:

1. `SearchGuest::execute(entry_id, ActionId::Open)` —
   `lib.rs:215`. The entry id is `network:<id>`;
   `query::parse_entry_id` returns `Some(id)`; control enters the
   `RUNTIME.with(|cell| ...)` block.

2. `RUNTIME.borrow_mut()` is held for the whole execute body. Two
   nested calls to `current_live_state(&runtime)` happen inside
   that borrow (one direct, one via `lookup_name_hint`). Each
   call constructs a fresh closure capturing a cloned `Client`
   and feeds it into `runtime.network_cache.get_or_fetch`.
   Borrow analysis says this is fine
   (`RefCell` interior mutability on the cache slot is a
   different cell from the `RUNTIME` `RefCell`), but a subtle
   re-entrancy bug here is the most plausible source of a
   host-visible crash. **Verify with a stripped-down reproducer
   that bypasses the launcher.**

3. `actions::connect(&client, &db, &network_id, &name_hint, now)`
   — `actions.rs`. Calls `client.join_network(id)` with body
   `b"{}"` and then `history::upsert_observed(db, &placeholder,
   now_ms)`. Both go through host imports
   (`http::fetch` + `sql::execute`). A type-shape mismatch in
   either WIT call would surface here.

4. `runtime.network_cache.invalidate()` after the action, while
   the outer `RUNTIME.borrow_mut()` is still held. Same
   double-RefCell pattern as #2. Independently low risk but
   worth eliminating.

## What to try first

- **Capture the actual panic.** Run
  `RUST_BACKTRACE=full just start` and reproduce. The host log
  should carry the panic site if it's a Rust panic.
- **Strip the launcher out of the equation.** Drop a unit test
  in `lib.rs` (or a small integration test in
  `src-tauri/tests/`) that calls `execute("network:<id>",
  ActionId::Open)` against an `httpmock` daemon and a fresh
  SQLite. Reproduce there to localize.
- **Suspect order:**
  1. Double `current_live_state` call inside one
     `RUNTIME.borrow_mut()` (cache + nested `name_hint`).
  2. `block_in_place` re-entrancy if `http::fetch` is somehow
     re-entered from a context that's already inside one.
  3. `actions::connect`'s placeholder Network serialization +
     `INSERT ... ON CONFLICT DO UPDATE` against a brand-new id.
  4. The launcher's frecency record on a synthetic entry id
     (`network:<id>` for an id with no prior history row) —
     `plugin_host.rs::execute` records frecency before
     dispatching; verify the frecency table accepts entries for
     unknown ids.

## Hypothesized fixes

Until the panic site is captured these are speculative:

- Lift the second `current_live_state` call out of
  `lookup_name_hint`. The synthetic-Connect path doesn't need a
  name hint at all — the network is by definition unknown; the
  daemon will fill in the name on the next refresh. Drop the
  call and use an empty placeholder name.
- Restructure `execute` so the host import calls happen
  *outside* the `RUNTIME.borrow_mut()` scope: clone what the
  match needs out of the runtime, drop the borrow, run the I/O,
  reacquire the borrow only for cache-invalidate.

The second is the right architectural answer regardless of
whether it fixes this specific bug — host import calls should
not be made under a long-lived borrow on shared plugin state.
The current shape works because WASM is single-threaded, but
it's fragile and "I/O while holding state" is a smell.

## Acceptance

- A user typing an unknown 16-hex-char id and pressing Enter
  joins the network, sees the entry transition to "Connecting…"
  on the next render, and the launcher does not crash.
- A regression test (host-side `httpmock` integration or
  plugin-side unit covering the relevant slice) covers the
  synthetic-Connect path.
- No panic surfaces in the host log on the join-by-id flow,
  even when the daemon is mid-state-change between observations.
