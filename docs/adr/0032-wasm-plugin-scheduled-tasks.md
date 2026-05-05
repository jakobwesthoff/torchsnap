# 32. WASM plugin scheduled tasks via host-managed cron scheduler

Date: 2026-04-08

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

Amended by [33. WASM plugin bridge owns instance lifecycle with compile-at-load and instantiate-on-enable](0033-wasm-plugin-bridge-owns-instance-lifecycle-with-compile-at-load-and-instantiate-on-enable.md)

## Context

WASM plugins can't spawn their own background threads — `wasm32-wasip2`
has no thread support. Native plugins like the calculator currently
spin up a dedicated `std::thread` with a `Condvar` to handle periodic
maintenance work (history retention cleanup every 30 minutes). The
calculator port to a WASM plugin needs an equivalent without that
mechanism.

The migration plan considered two alternatives:

1. **Lazy cleanup at every entry point** — every `search()` or
   `handle_message` call checks an atomic timestamp and runs cleanup
   if enough time has elapsed.
1. **A host-managed scheduler** — the host runs a per-plugin tokio
   loop that calls a guest export at the configured cadence.

Option (1) was rejected because it pushes throttling state into every
plugin (atomic timestamps + CAS), scatters cleanup-trigger code across
every entry point, and never runs when the plugin is idle. "WASM has
no threads" is a real capability gap that the host should fill once,
generically.

## Decision

Add a host-managed scheduler driven by `[[tasks]]` entries in
`manifest.toml`. Each entry declares a stable `id` and a 5-field
POSIX cron expression. The host's `WasmPluginBridge` spawns a single
tokio task per plugin (only when the manifest declares at least one
`[[tasks]]` entry) that walks every schedule, sleeps until the
earliest next fire across all of them, and invokes the
`tasks::run-task` WIT guest export.

**WIT additions** (`plugins/plugin-sdk/wit/torchsnap-plugin.wit`):

````wit
interface tasks {
  /// Invoked by the host at the cron-scheduled time for the
  /// task with the given manifest-declared `task-id`.
  run-task: func(task-id: string) -> result<_, string>;
}

world plugin {
  ...
  export tasks;
}
````

**Manifest** (`src-tauri/src/wasm/manifest.rs`):

````toml
[[tasks]]
id = "retention-cleanup"
schedule = "*/30 * * * *"   # 5-field POSIX cron
````

`Manifest::parse` validates every entry at plugin load time:

* Schedules parse via the `cron` crate (the validation strategy is
  documented below).
* Task ids must be unique within a plugin.

Both checks fail plugin load on violation — broken schedules surface
as clean errors instead of crashing the scheduler later.

### Cron format: 5-field POSIX, validated by parser wrapping

The `cron` crate's `Schedule::from_str` accepts only Quartz-style 6
or 7-field syntax (`sec min hour day month dow [year]`) — POSIX 5-field
is below its minimum. We need a way to enforce strict 5-field POSIX
input and reject anything else, including the 6/7-field forms that
would let plugins smuggle in second-level scheduling.

The trick: wrap the user's input as `format!("0 {schedule} *")` before
handing it to `cron`. This produces:

* Valid 5-field POSIX → 7-field Quartz (`0` for seconds, `*` for
  year) → `cron` accepts.
* 6-field input → 8-field intermediate → `cron` rejects.
* 7-field input → 9-field intermediate → `cron` rejects.
* Garbage tokens → still wrong field count → `cron` rejects.

So the wrapping itself is the validation — no separate field counter,
no duplicated logic. `cron` reports its own error message which
flows up through the manifest validator's
`anyhow::anyhow!("invalid schedule for task `{id}`: {e}")` wrapper.

A direct alternative — translate 5→6 by prepending `0 ` only — was
rejected because it accepts both 5-field and 6-field input, and the
6-field path lets plugins choose a non-`0` seconds value, which gets
us back to sub-minute scheduling.

### Per-plugin scheduler: one tokio task per plugin

The scheduler runs **one tokio task per plugin**, not one per
`[[tasks]]` entry. The wasmtime store mutex
(`WasmPluginInstance::store`) already serializes every guest call,
so spawning N tokio tasks for N schedules would pay tokio overhead
for zero added concurrency. A single per-plugin loop walks all
declared tasks, sleeps until the earliest next fire, and fires
whichever tasks are due in declaration order.

````rust
async fn scheduler_loop(
    instance: Arc<WasmPluginInstance>,
    schedules: Vec<(String, Schedule)>,
    log_sender: LogSender,
    plugin_id: String,
) {
    loop {
        let now = Utc::now();

        // Snapshot next-fire per task. Sleeping past the
        // snapshots tells us unambiguously which to fire on
        // wake — re-querying after sleep would skip
        // everything because cron returns *future* fires.
        let next_fires: Vec<(usize, DateTime<Utc>)> = schedules
            .iter()
            .enumerate()
            .filter_map(|(i, (_, sched))| sched.after(&now).next().map(|t| (i, t)))
            .collect();

        let Some(earliest) = next_fires.iter().map(|(_, t)| *t).min() else {
            return;
        };

        let delta = (earliest - now).to_std().unwrap_or_default();
        if !delta.is_zero() {
            tokio::time::sleep(delta).await;
        }

        for (i, fire) in &next_fires {
            if *fire != earliest { continue; }
            let (id, _) = &schedules[*i];
            match instance.run_task(id) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log_task_error(&log_sender, &plugin_id, id, &e),
                Err(e) => log_task_error(&log_sender, &plugin_id, id, &format!("{e:#}")),
            }
        }
    }
}
````

Properties:

* **One tokio task per plugin** regardless of how many `[[tasks]]`
  the plugin declares. Zero tokio overhead for plugins without tasks
  (the loop is never spawned).
* **Sequential execution** within the plugin is automatic. Tasks
  scheduled at the same wall-clock instant run back-to-back in
  manifest declaration order.
* **Re-querying after every wake** — the loop recomputes the next
  fire from the current time on every iteration, so clock jumps,
  laptop sleep/wake, and DST transitions are handled by `cron`'s
  own iterator semantics. No manual time math.
* **One handle to abort on disable**, no per-task tracking, no
  HashMap of running flags.

### Bridge wiring

`WasmPluginBridge` gains:

* `instance: Arc<WasmPluginInstance>` (was previously owned —
  needed `Arc` so the spawned tokio task can also hold a reference).
* `parsed_tasks: Vec<ParsedTask>` — the manifest's `[[tasks]]`
  entries with their cron schedules already parsed at construction
  time.
* `scheduler_handle: Mutex<Option<JoinHandle<()>>>` — the running
  scheduler tokio handle, swappable on enable/disable.

`enable()` calls `spawn_scheduler()` after the guest's own `enable()`
runs. `disable()` calls `stop_scheduler()` BEFORE tearing down
settings/storage handles, so the scheduler can't fire one last task
into a half-disabled plugin.

`WasmPluginInstance::run_task` is the new typed wrapper around the
WIT export, mirroring the existing `enable`/`disable`/`entries`/etc.
wrappers.

### Locked-in semantics

* **Cron format**: 5-field POSIX only (enforced by parser wrapping).
* **First fire**: at the next cron match only. No automatic
  immediate fire on enable. Plugins that want startup work do it
  explicitly in their own `enable()`.
* **Failure policy**: log and continue. Both wasmtime traps and
  plugin-reported `Err(string)` are logged at error level via the
  bridge's `log_task_error` helper. No consecutive-failure counter,
  no auto-disable. YAGNI for v1.
* **Persistence of missed fires across launches**: not supported.
  Standard cron behavior — if the launcher is closed for 4 hours and
  a task was supposed to fire 8 times, it does **not** "make up"
  the missed fires on next launch.
* **Schedule validation timing**: at manifest parse time. Refuses
  to load plugins with malformed cron expressions.
* **Default `run-task` for plugins without tasks**: every plugin must
  implement the WIT export (it's a hard guest contract), but
  hello-world and the calculator template-copy ship `Ok(())`
  no-ops because they declare no `[[tasks]]`. The host never spawns
  a scheduler loop for those plugins so the no-op is never called.

## Alternatives considered

* **Lazy cleanup at every entry point** — rejected as discussed in
  Context. Calculator-specific, scattered, no idle execution.
* **One tokio task per `[[tasks]]` entry** — rejected. The wasmtime
  store mutex already serializes guest calls, so multiple tokio
  tasks add overhead with no concurrency win.
* **Sub-minute scheduling** — rejected. Predictability matters more
  than precision. Plugins that genuinely need sub-second timing
  should run on the native side.
* **One-shot delayed tasks** — out of scope for v1. `[[tasks]]` is
  for recurring schedules only.
* **External cron triggering** (host force-runs a task on demand
  outside its schedule) — out of scope. Plugins that need this
  expose it via `handle-message`.
* **Persistence of missed fires across launches** — out of scope.
  Adding requires cron state persistence and behavior is footgun-y
  (a plugin could fire 100 times in a row on first launch after a
  long absence). Standard cron behavior.

## Consequences

* WASM plugins get periodic background work without threads. The
  calculator port can declare `[[tasks]] retention-cleanup` and
  delete its native dedicated cleanup thread.
* Adding a new lifecycle/messaging-style guest export
  (`tasks::run-task`) is a WIT-breaking change for any existing
  guest. Every shipped plugin must implement it (even as a no-op
  `Ok(())`) or fail to compile against the new world. The
  `template`, `hello-world`, and untracked `calculator` template-copy
  crates were updated in the same commit.
* The cron format strictness is enforced at the validation layer by
  the wrapping format string — no separate counter, no duplicated
  field-count check, less code to maintain. The trade-off is that
  the error message comes from `cron` and isn't quite as friendly
  as a hand-rolled "expected 5-field POSIX" check would be. We can
  add a friendlier wrapper if real plugin authors hit it.
* The scheduler holds an `Arc<WasmPluginInstance>` for the duration
  of a plugin's enabled lifetime. `disable()` aborts the tokio task
  before clearing the rest of `PluginState`, so there's no risk of
  the scheduler firing into a partially-disabled plugin.