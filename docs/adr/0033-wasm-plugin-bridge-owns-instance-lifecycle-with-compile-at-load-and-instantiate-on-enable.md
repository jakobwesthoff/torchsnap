# 33. WASM plugin bridge owns instance lifecycle with compile-at-load and instantiate-on-enable

Date: 2026-04-11

## Status

Accepted
Amends [31. WASM plugin SQL storage API](0031-wasm-plugin-sql-storage-api.md)
Amends [32. WASM plugin scheduled tasks via host-managed cron scheduler](0032-wasm-plugin-scheduled-tasks.md)

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

Before this ADR, the WASM plugin loader worked like this:

1. `lib.rs::load_single_wasm_plugin` opens the plugin source.
1. It reads the WASM bytes and calls `WasmRuntime::instantiate(&id, &bytes)`
   to compile the component and create a `WasmPluginInstance` in one step.
1. It hands the finished instance to `WasmPluginBridge::new`, which
   stashes the manifest-derived SQL config on the live store and keeps
   an `Arc<WasmPluginInstance>` as a bridge field.
1. The bridge's `disable()` calls the guest's `lifecycle::disable`, clears
   settings / SQL / clipboard state, but leaves the instance allocated.

This layout has two concrete problems:

**Disabled plugins consume memory.** The instance — and its wasmtime
`Store`, linear memory, resource table, and every byte of guest state —
is allocated at load time regardless of whether the user wants the plugin
enabled. The canonical example is the hello-world plugin's 50,000-entry
petname table: tens of megabytes of heap that stay resident for the life
of the app even though the plugin never runs. A user who installs ten
plugins and enables two still pays the full compile + instantiate + store
cost for all ten on every app launch, and the eight disabled ones sit in
memory for the whole session.

**The layering is wrong for destroy/recreate.** The ingredients needed to
recreate an instance (the runtime, the WASM bytes, the SQL config) live
outside the thing that manages its lifecycle (the bridge). Fixing the
memory waste by adding a "drop on disable, reinstantiate on enable" path
means pulling those ingredients into the bridge anyway — which turns out
to be the right layering in the first place. The old split made sense
only because instance lifetime equalled bridge lifetime; once they
diverge, putting instance creation outside the bridge's walls is an
artificial separation that forces the recreate path through awkward
indirection.

## Decision

Move full ownership of a WASM plugin's instance lifecycle into
`WasmPluginBridge`. The bridge becomes the host-side representation of a
WASM plugin: it compiles the component at construction, lazily creates
a fresh `WasmPluginInstance` on enable, and drops it on disable so the
wasmtime `Store` is reclaimed.

### Split `WasmRuntime` API: compile + instantiate

`WasmRuntime::instantiate(plugin_id, &bytes)` — which did both
compilation and instantiation in one step — is replaced by two separate
entry points:

````rust
impl WasmRuntime {
    pub fn compile(&self, plugin_id: &str, wasm_bytes: &[u8]) -> anyhow::Result<()>;
    pub fn instantiate(&self, plugin_id: &str) -> anyhow::Result<WasmPluginInstance>;
}
````

`compile` parses the component-model binary, validates it, and stores
the resulting `wasmtime::Component` in a per-plugin cache
(`Mutex<HashMap<String, Component>>`) on the runtime. `instantiate`
reads the cached `Component`, builds a fresh linker + `WasiCtx` +
`PluginState` + `Store`, and returns a ready-to-call
`WasmPluginInstance`. Calling `instantiate` for a plugin ID that has
not been compiled returns an error — it is an internal programming bug,
not a user-facing failure mode.

The split is load-bearing: it lets the expensive step (compilation,
~100ms for a typical plugin) run once at load time while the cheap
step (instantiation, sub-millisecond against a cached component) can
be repeated on every enable toggle without re-paying the compile cost.

`WasmRuntime::new` now returns `Arc<Self>` so the single
per-application runtime can be cloned cheaply into every bridge that
needs a handle for on-demand instantiation.

### Bridge owns the lifecycle

`WasmPluginBridge::new` signature changes from accepting a finished
`WasmPluginInstance` to accepting the ingredients needed to create one:

````rust
pub fn new(
    manifest: Manifest,
    runtime: Arc<WasmRuntime>,
    log_sender: LogSender,
    source: &dyn PluginSource,
    app_data_dir: &Path,
) -> anyhow::Result<Self>;
````

The constructor does three things:

1. Reads the WASM bytes from the source, calls `runtime.compile(...)`,
   drops the bytes. Broken components fail plugin load at this step.
1. Reads SQL migration files from the source (the logic that ADR 0031
   put in this constructor), materializes `SqlConfig`, and stores it
   as a bridge field. No live instance is involved here; the config is
   re-applied to every fresh `PluginState` later.
1. Pre-parses cron schedules from `[[tasks]]`.

It does **not** instantiate. The `instance` field is
`Mutex<Option<Arc<WasmPluginInstance>>>` and starts as `None`. Disabled
plugins therefore consume only their cached `Component` — no wasmtime
`Store`, no linear memory, no resource tables — until the user turns
them on.

`Plugin::enable` goes through a new `ensure_instance` helper that locks
the slot, calls `runtime.instantiate(&plugin_id)` if empty, re-applies
the cached `sql_config` to the fresh `PluginState`, and returns an
`Arc<WasmPluginInstance>` clone. The caller then stashes settings /
clipboard / SQL handles, calls the guest's own `enable()`, and starts
the scheduler — all against the cloned `Arc`, with the outer mutex
released before any guest call runs.

`Plugin::disable` stops the scheduler first (so its `Arc` clone is
released before we check the strong count), then `take_instance` clears
the slot, runs the teardown calls on the taken `Arc`, and lets it drop.
Once the scheduler clone and the local are gone, the strong count hits
zero and the wasmtime `Store` is freed.

### Lock discipline

`Mutex<Option<Arc<WasmPluginInstance>>>` has a strict rule: **never
call into the guest while holding the mutex**. Every access is
`lock → Arc::clone → drop lock → call`. The `WasmPluginInstance`'s
inner `Mutex<Store<...>>` still serializes guest calls on its own; the
outer lock exists only to cover the "create / drop the slot"
transitions driven by `enable()` and `disable()`. Holding the outer
lock across a guest call would deadlock if a second trait method
(e.g. a reentrant `handle_message`) tried to acquire it.

### Guest `enable()` failure drops the instance

If `WasmPluginInstance::enable()` (the guest call, not the bridge's
`Plugin::enable`) returns `Err` — typically because the guest's own
initialization code panicked and wasmtime converted the panic into a
trap — the bridge logs the error and drops the instance back to `None`.
Rationale: a guest that failed its own `enable()` hook is not in a
useful state, and "alive but not enabled" is not a coherent state to
preserve.

The bridge cannot propagate this failure to the host's
`AtomicBool enabled` flag today: `Plugin::enable` returns `()`. The
observable consequence is that the settings UI still shows the plugin
as enabled while every subsequent dispatch hits the defensive `None`
branch in the trait methods and no-ops with a warning. Fixing this
properly requires extending the `Plugin` trait so `enable` can return
a `Result` the host reacts to; that change is tracked as a separate
follow-up todo under `todos/wasm/` because it touches every plugin
impl in the repo.

### `load_single_wasm_plugin` becomes thin

`lib.rs::load_single_wasm_plugin` collapses to "open source, construct
bridge, register with host, insert source into registry". The
`read_wasm()` / `runtime.compile()` / `runtime.instantiate()` calls all
moved into the bridge constructor. The source is still retained in
`PluginSourceRegistry` for the asset-serving protocol handler, but the
bridge does not keep its own reference — every byte the bridge needs
from the source is already materialized by the time the constructor
returns.

## Alternatives considered

* **Keep the existing layering; just add a "drop instance" path on
  disable.** Rejected. It still requires pulling the runtime handle
  and SQL config into the bridge to re-create the instance on re-enable,
  and the asymmetry between "instance created by `lib.rs`" and
  "instance recreated by the bridge" makes the lifecycle harder to
  reason about than if the bridge owns both sides.

* **Drop the whole bridge on disable and rebuild it on re-enable.**
  Considered (this was the first instinct during the planning
  discussion). Rejected because `PluginHost` holds bridges as
  `Box<dyn Plugin>` in a slot keyed by ID — unregistering and
  re-registering at the host level would require a cross-cutting
  change to the host's plugin map and the enable/disable dispatch
  path, and the memory savings are the same. The bridge's "cold
  state" (manifest, parsed tasks, SQL config) is tiny compared to the
  live `Store`; dropping just the instance captures ~99% of the
  benefit for a fraction of the code change.

* **Compile lazily on first enable instead of at load time.** Rejected.
  Compiling at load time surfaces broken plugins as clean load errors,
  which is more useful than discovering the breakage on first enable.
  The memory cost of a cached `Component` is small (native code for
  the guest's imports/exports, no linear memory) relative to a live
  `Store`, so keeping the compiled component around for disabled
  plugins is worth the upfront cost for the early error surface alone.

* **Cache the `Linker<PluginState>` alongside the `Component`.**
  Considered. Rejected. Linker construction is microseconds — it
  inserts host function pointers into a map, no WASM compilation — and
  `instantiate` is not called in a hot loop. The added field and
  lifetime management would be complexity without observable benefit.

## Consequences

* **Disabled plugins no longer allocate a `Store`.** The 50k-petnames
  hello-world scenario drops from tens of megabytes to the compiled
  `Component` footprint alone (a few kilobytes).
* **Toggle off → on is fast.** The cached `Component` means re-enable
  runs the instantiation path only, which is cheap enough to run on
  every user toggle without lag.
* **Re-enable runs guest `enable()` on fresh state.** This is the
  intended semantic: a plugin that needs cross-disable persistence
  must use SQL storage (ADR 0031), not in-memory state on the guest
  side. Plugin authors who were relying on in-memory state surviving
  disable cycles would now see it reset; none of the shipped plugins
  depend on that behaviour.
* **Guest `enable()` failures leave the host's `enabled` flag stale
  until the follow-up `Plugin::enable → Result` refactor lands.**
  Observable effect: a plugin that fails its own initialization shows
  as enabled in the settings UI but its trait methods no-op. This is
  worse than today's silent half-state because there is no ambiguity
  about what happened (the bridge logged a clear error), but the UI
  reconciliation is explicitly deferred.
* **ADR 0031's migration-file reads still happen at bridge
  construction time**, just via the new constructor signature. The
  SQL config lives on the bridge now instead of being stashed on the
  first (and only) `PluginState`, which is the shape required to
  re-apply it on every fresh instance.
* **ADR 0032's scheduler semantics are unchanged.** The scheduler
  still holds an `Arc<WasmPluginInstance>` and `stop_scheduler` still
  runs before the bridge drops its own clone on disable. The
  ordering invariant ADR 0032 documented — "stop the scheduler before
  tearing down settings/storage" — becomes load-bearing in a new
  way: without it, the scheduler's clone would keep the `Store`
  alive past the bridge's slot clearing, defeating memory
  reclamation. The existing `stop_scheduler` call at the top of
  `Plugin::disable` already enforces this; no code change in the
  scheduler itself.
* **No changes to the WIT interface or guest-side contract.** This
  is an entirely host-side refactor.