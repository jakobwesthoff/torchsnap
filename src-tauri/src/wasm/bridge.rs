// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Plugin Bridge
//
// Host-side representation of a WASM plugin. Owns the
// plugin's full lifecycle: compiles the component at
// construction, lazily instantiates a `WasmPluginInstance`
// on enable, and drops it on disable so the wasmtime
// `Store` and all guest-side linear memory are reclaimed.
// Implements the native `Plugin` trait so WASM plugins
// participate in the existing `PluginHost` dispatch
// pipeline alongside native plugins.
// =========================================================

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use anyhow::Context;
use chrono::Utc;
use cron::Schedule;
use tauri::async_runtime::JoinHandle;

use crate::plugins::Plugin;
use crate::search::types::{ActionId, CatalogEntry, PluginResponse, PostAction};
use crate::settings::SettingsInit;

use super::logging::channel::LogSender;
use super::logging::{LogItem, LogItemKind, LogLevel, LogSource};
use super::manifest::Manifest;
use super::runtime::{SqlConfig, WasmPluginInstance, WasmRuntime};
use super::source::PluginSource;

// =========================================================
// WasmPluginBridge
// =========================================================

/// Host-side representation of a WASM plugin. Owns the
/// manifest, a handle to the shared `WasmRuntime`, the
/// materialized `SqlConfig`, and the currently live
/// instance slot.
pub struct WasmPluginBridge {
    manifest: Manifest,
    plugin_id: String,
    runtime: Arc<WasmRuntime>,
    /// Re-applied to every fresh `WasmPluginInstance` on
    /// enable — each new `PluginState` starts with
    /// `SqlConfig::None`, so the bridge holds the
    /// materialized config here to survive disable/re-enable
    /// cycles without re-reading the plugin source.
    sql_config: SqlConfig,
    /// Live guest instance, or `None` while disabled.
    ///
    /// **Lock discipline**: never call into the guest while
    /// holding this lock. Every access is
    /// `lock → Arc::clone → drop lock → call`, so the outer
    /// lock only covers slot creation / teardown. The
    /// `WasmPluginInstance`'s inner store mutex is what
    /// serializes guest calls.
    instance: Mutex<Option<Arc<WasmPluginInstance>>>,
    log_sender: LogSender,
    /// Pre-parsed `[[tasks]]` entries from the manifest. The
    /// raw schedule strings are validated at manifest load
    /// time; this list holds the parsed `cron::Schedule`s
    /// alongside their ids ready to be consumed by the
    /// scheduler loop in `enable()`.
    parsed_tasks: Vec<ParsedTask>,
    /// Tokio handle for the running scheduler loop. `None`
    /// while the plugin is disabled or when the plugin has
    /// no `[[tasks]]` declared. The `Mutex` covers the
    /// `enable()` / `disable()` swap, not the loop itself.
    scheduler_handle: Mutex<Option<JoinHandle<()>>>,
    /// Co-operative shutdown flag the scheduler loop
    /// checks before each `run_task` invocation.
    /// `tokio::JoinHandle::abort()` only takes effect at
    /// the next `await` point — for the scheduler that
    /// is the next `tokio::time::sleep`. Without this
    /// flag, an in-flight `run_task` would still finish
    /// against a half-disabled plugin between
    /// `stop_scheduler()` and the actual abort. Setting
    /// the flag in `stop_scheduler` and checking it both
    /// before and after every guest call closes the
    /// window.
    scheduler_shutdown: Arc<AtomicBool>,
}

/// A task definition with its cron schedule already parsed.
struct ParsedTask {
    id: String,
    schedule: Schedule,
}

impl WasmPluginBridge {
    /// Build a bridge from a parsed manifest, a handle to
    /// the shared runtime, and the plugin source (used
    /// once to read the WASM bytes and any SQL migration
    /// files). `app_data_dir` is the host's per-app data
    /// root; the plugin's database lives at
    /// `<app_data_dir>/plugins/<plugin-id>/storage.db`.
    ///
    /// Compiles the WASM component into the runtime's
    /// cache right here so broken plugins fail fast at
    /// load time. Does **not** instantiate — a fresh
    /// `WasmPluginInstance` is created later on demand by
    /// `Plugin::enable`, so disabled plugins consume only
    /// their cached `Component` until the user turns them
    /// on.
    pub fn new(
        manifest: Manifest,
        runtime: Arc<WasmRuntime>,
        log_sender: LogSender,
        source: &dyn PluginSource,
        app_data_dir: &std::path::Path,
    ) -> anyhow::Result<Self> {
        let plugin_id = manifest.plugin.id.as_str().to_string();

        // Compile the component into the runtime's cache
        // once; every subsequent `instantiate` reads from
        // there. A failure here surfaces as a plugin load
        // error (manifest bug or broken build).
        runtime
            .compile(&plugin_id, &source.read_wasm()?)
            .with_context(|| format!("compile WASM component for `{plugin_id}`"))?;

        // Materialize the SQL configuration from the
        // manifest. Plugins without `[storage.sql]` get
        // `SqlConfig::None`; the bridge's `enable()` path
        // is a no-op for database setup in that case.
        let sql_config = match manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
            None => SqlConfig::None,
            Some(sql) => {
                let mut migrations: Vec<String> = Vec::with_capacity(sql.migrations.len());
                for path in &sql.migrations {
                    let bytes = source
                        .read_file(path)
                        .with_context(|| format!("read SQL migration file `{path}`"))?;
                    let text = String::from_utf8(bytes).with_context(|| {
                        format!("SQL migration file `{path}` is not valid UTF-8")
                    })?;
                    migrations.push(text);
                }

                let db_path: PathBuf = app_data_dir
                    .join("plugins")
                    .join(plugin_id.as_str())
                    .join("storage.db");

                SqlConfig::Configured {
                    db_path,
                    migrations: Arc::new(migrations),
                }
            }
        };

        // Pre-parse every `[[tasks]]` schedule. The manifest
        // loader has already validated that they're well-
        // formed 5-field POSIX cron expressions, so this
        // re-parse is purely a unwrap-safe conversion to the
        // `cron::Schedule` form the scheduler loop wants.
        let parsed_tasks = manifest
            .tasks
            .iter()
            .map(|task| {
                let normalized = format!("0 {} *", task.schedule);
                let schedule = Schedule::from_str(&normalized).with_context(|| {
                    format!(
                        "internal: parsing pre-validated cron schedule `{}` for task `{}`",
                        task.schedule, task.id
                    )
                })?;
                Ok(ParsedTask {
                    id: task.id.clone(),
                    schedule,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Self {
            manifest,
            plugin_id,
            runtime,
            sql_config,
            instance: Mutex::new(None),
            log_sender,
            parsed_tasks,
            scheduler_handle: Mutex::new(None),
            scheduler_shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Return a clone of the live instance, creating one
    /// via `runtime.instantiate` if the slot is empty.
    /// Re-applies the bridge's `sql_config` on every fresh
    /// instance — each new `PluginState` starts with
    /// `SqlConfig::None`.
    fn ensure_instance(&self) -> anyhow::Result<Arc<WasmPluginInstance>> {
        let mut slot = self.instance.lock().expect("instance slot not poisoned");
        if let Some(existing) = slot.as_ref() {
            return Ok(Arc::clone(existing));
        }

        let instance = self
            .runtime
            .instantiate(&self.plugin_id)
            .with_context(|| format!("instantiate WASM plugin `{}`", self.plugin_id))?;
        instance.set_sql_config(self.sql_config.clone());

        let arc = Arc::new(instance);
        *slot = Some(Arc::clone(&arc));
        Ok(arc)
    }

    /// Take the instance out of the slot. The caller must
    /// drop every other `Arc` clone (notably the scheduler
    /// task's) before the underlying `Store` can be freed.
    fn take_instance(&self) -> Option<Arc<WasmPluginInstance>> {
        self.instance
            .lock()
            .expect("instance slot not poisoned")
            .take()
    }

    /// Spawn the per-plugin scheduler tokio task that walks
    /// every `[[tasks]]` entry, sleeps until the earliest
    /// next fire across all of them, and invokes the WIT
    /// `tasks::run-task` guest export.
    ///
    /// The `instance` argument is passed in by
    /// `Plugin::enable` from the already-cloned `Arc` it
    /// holds — the scheduler task needs its own clone so
    /// it can keep the instance alive for as long as the
    /// task is running, independent of the bridge's
    /// instance slot.
    ///
    /// Plugins without `[[tasks]]` get nothing — no tokio
    /// task is spawned at all, so the cost is zero for
    /// plugins that don't use the API.
    fn spawn_scheduler(&self, instance: Arc<WasmPluginInstance>) {
        if self.parsed_tasks.is_empty() {
            return;
        }

        // Reset the shutdown flag — a previous disable
        // cycle would have set it to true. Without this
        // reset, the freshly-spawned scheduler would exit
        // immediately on its first iteration.
        self.scheduler_shutdown.store(false, Ordering::Relaxed);

        // Snapshot what the scheduler loop needs into Send
        // clones — the parsed schedules, the log sender,
        // the shutdown flag, and a plugin id for log
        // tagging. The `instance` clone is already held by
        // the caller and handed to us by value.
        let shutdown = Arc::clone(&self.scheduler_shutdown);
        let log_sender = self.log_sender.clone();
        let plugin_id = self.plugin_id.clone();
        let schedules: Vec<(String, Schedule)> = self
            .parsed_tasks
            .iter()
            .map(|t| (t.id.clone(), t.schedule.clone()))
            .collect();

        let handle = tauri::async_runtime::spawn(async move {
            scheduler_loop(instance, schedules, shutdown, log_sender, plugin_id).await;
        });

        let mut slot = self.scheduler_handle.lock().expect("not poisoned");
        // If a previous scheduler is somehow still running
        // (re-enable without disable in between), abort it
        // before installing the new one to avoid duplicate
        // fires.
        if let Some(prev) = slot.take() {
            prev.abort();
        }
        *slot = Some(handle);
    }

    /// Signal the running scheduler to stop and abort the
    /// tokio task. Called from `disable()`.
    ///
    /// `JoinHandle::abort()` only takes effect at the next
    /// `await` point — for the scheduler that's the next
    /// `tokio::time::sleep`. The shutdown flag is the
    /// co-operative early-exit path: the loop checks it
    /// both at the top of every iteration and immediately
    /// before each `run_task` call, so an in-flight
    /// guest call finishes cleanly and the next would-be
    /// invocation is suppressed even if `abort()` hasn't
    /// taken hold yet.
    fn stop_scheduler(&self) {
        self.scheduler_shutdown.store(true, Ordering::Relaxed);
        if let Some(handle) = self.scheduler_handle.lock().expect("not poisoned").take() {
            handle.abort();
        }
    }
}

// =========================================================
// Per-plugin scheduler loop
//
// Sequential by construction: a single tokio task walks
// every `[[tasks]]` entry, sleeps until the earliest next
// fire across all of them, and invokes the WIT
// `tasks::run-task` guest export. The wasmtime store mutex
// already serializes guest calls, so spawning N tokio tasks
// for N schedules would pay tokio overhead for zero added
// concurrency.
//
// Re-querying every iteration handles clock jumps,
// laptop sleep/wake, and DST naturally — no manual time
// math.
// =========================================================

async fn scheduler_loop(
    instance: Arc<WasmPluginInstance>,
    schedules: Vec<(String, Schedule)>,
    shutdown: Arc<AtomicBool>,
    log_sender: LogSender,
    plugin_id: String,
) {
    loop {
        // Top-of-iteration shutdown check. Exits cleanly
        // if `stop_scheduler` set the flag while we were
        // sleeping or after a long-running guest call.
        if shutdown.load(Ordering::Relaxed) {
            return;
        }

        // Snapshot the next-fire time for every task
        // relative to "now" at the top of this iteration.
        // Sleeping past those snapshots tells us
        // unambiguously which tasks should fire after the
        // wake — comparing against re-queried `upcoming`
        // results would skip everything because cron
        // returns *future* fires only.
        let now = Utc::now();
        let next_fires: Vec<(usize, chrono::DateTime<Utc>)> = schedules
            .iter()
            .enumerate()
            .filter_map(|(i, (_, sched))| sched.after(&now).next().map(|t| (i, t)))
            .collect();

        let Some(earliest) = next_fires.iter().map(|(_, t)| *t).min() else {
            // No schedules left to fire (e.g. every cron
            // expression has exhausted itself, which the
            // `cron` crate doesn't actually do — but be
            // defensive).
            return;
        };

        let delta = (earliest - now).to_std().unwrap_or_default();
        if !delta.is_zero() {
            tokio::time::sleep(delta).await;
        }

        // Fire every task whose snapshotted next-fire
        // matched the earliest. Multiple tasks can share
        // the same fire time (e.g. `0 * * * *` and
        // `*/30 * * * *` both at :00) — they run back-to-
        // back in manifest declaration order.
        for (i, fire) in &next_fires {
            if *fire != earliest {
                continue;
            }

            // Per-task shutdown check. Catches the race
            // where `stop_scheduler` set the flag while
            // we were inside the previous task's guest
            // call (which holds the wasmtime store mutex
            // and can't be interrupted by `JoinHandle::abort`).
            if shutdown.load(Ordering::Relaxed) {
                return;
            }

            let (id, _) = &schedules[*i];

            // The guest call is synchronous from the tokio task's
            // perspective — wasmtime stores are blocking. We call it
            // inline here rather than wrapping in `spawn_blocking`
            // because v1 task workloads are tiny (the calculator's
            // retention cleanup is a single SQL DELETE that takes
            // sub-millisecond time). If a plugin author writes a
            // pathological task — e.g. a multi-second web scrape or a
            // large file scan — it will block one tokio worker
            // thread for the duration. With the default Tauri
            // runtime (4 worker threads), four such plugins
            // scheduling simultaneously would saturate the runtime.
            // The fix when this happens is to wrap `instance.run_task`
            // in `tokio::task::spawn_blocking(...).await?`; nothing
            // about the guest interface needs to change.
            match instance.run_task(id) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log_task_error(&log_sender, &plugin_id, id, &e),
                Err(e) => log_task_error(&log_sender, &plugin_id, id, &format!("{e:#}")),
            }
        }
    }
}

fn log_task_error(log_sender: &LogSender, plugin_id: &str, task_id: &str, error: &str) {
    log_sender.send(LogItem {
        seq: 0,
        timestamp: SystemTime::now(),
        source: LogSource::Plugin(plugin_id.to_string()),
        kind: LogItemKind::Message {
            level: LogLevel::Error,
            message: format!("scheduled task `{task_id}` failed: {error}"),
            metadata: vec![
                ("task_id".to_string(), task_id.to_string()),
                ("error".to_string(), error.to_string()),
            ],
            span_id: None,
        },
    });
}

impl WasmPluginBridge {
    /// Emit a log entry for bridge-level events (errors from
    /// guest calls that are caught and handled here).
    fn log(&self, level: LogLevel, message: String) {
        self.log_sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: LogSource::Plugin(self.plugin_id.clone()),
            kind: LogItemKind::Message {
                level,
                message,
                metadata: vec![],
                span_id: None,
            },
        });
    }

    /// Clone the live instance out of the slot for a guest
    /// call. Returns `None` only when the host dispatches
    /// into a disabled bridge (shouldn't happen under the
    /// current `AtomicBool` gating, but can occur
    /// transiently if the bridge tore its instance down
    /// after a guest `enable()` failure — see the
    /// `Plugin::enable → Result` follow-up todo).
    fn current_instance(&self) -> Option<Arc<WasmPluginInstance>> {
        self.instance
            .lock()
            .expect("instance slot not poisoned")
            .as_ref()
            .map(Arc::clone)
    }

    /// Log a "bridge dispatched while disabled" event at
    /// `Error` level with a uniform `bug:` prefix. This
    /// state should never occur under the host's
    /// `AtomicBool` gating — if it does, it is either a
    /// genuine dispatch bug or the known transient
    /// post-guest-enable-failure window described in the
    /// `Plugin::enable → Result` follow-up todo. Once
    /// that follow-up lands and the window is eliminated,
    /// the log calls at the call sites should be replaced
    /// by `debug_assert!(false, ...)` or an outright
    /// `unreachable!()` — the code path is provably
    /// dead at that point.
    fn log_dispatched_while_disabled(&self, method: &str) {
        self.log(
            LogLevel::Error,
            format!("bug: {method} dispatched on disabled plugin"),
        );
    }
}

impl Plugin for WasmPluginBridge {
    fn id(&self) -> &str {
        self.manifest.plugin.id.as_str()
    }

    /// Write the `[settings]` table defaults from the manifest to
    /// the settings store. Each entry is an `ensure()` call —
    /// existing user values are never overwritten.
    fn initialize_settings(&self, mut settings: SettingsInit) -> SettingsInit {
        for (key, toml_value) in &self.manifest.settings {
            if let Ok(json_value) = serde_json::to_value(toml_value) {
                settings = settings.ensure(key, json_value);
            }
        }
        settings
    }

    fn enable(&self, app: &tauri::AppHandle, ctx: &crate::plugins::PluginContext) {
        let instance = match self.ensure_instance() {
            Ok(instance) => instance,
            Err(e) => {
                self.log(
                    LogLevel::Error,
                    format!("failed to instantiate plugin: {e:#}"),
                );
                return;
            }
        };

        // Stash settings BEFORE invoking the guest's
        // `enable()` so the guest can call `settings::get`
        // during its own initialization.
        instance.set_settings(ctx.settings.clone());

        let app_handle = app.clone();
        instance.set_clipboard_writer(Box::new(move |text| {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            app_handle
                .clipboard()
                .write_text(text)
                .map_err(|e| format!("write to clipboard: {e}"))
        }));

        // Materialize the SQL database before the guest's
        // enable() runs. Failure here leaves the guest in a
        // bad state (any `sql::connection()` call would
        // panic in the host import), so tear the instance
        // back down before returning.
        if let Err(e) = instance.open_sql_storage() {
            self.log(LogLevel::Error, format!("SQL storage init failed: {e:#}"));
            drop(instance);
            let _ = self.take_instance();
            return;
        }

        // A guest that failed its own `enable()` is not in
        // a useful state, so drop the instance and skip
        // the scheduler. The host's enabled flag stays
        // `true` until the `Plugin::enable → Result`
        // follow-up gives us a channel to report failure
        // upward.
        if let Err(e) = instance.enable() {
            self.log(LogLevel::Error, format!("guest enable() failed: {e:#}"));
            drop(instance);
            let _ = self.take_instance();
            return;
        }

        self.spawn_scheduler(instance);
    }

    fn disable(&self) {
        // Stop the scheduler before touching the instance:
        // it holds its own `Arc` clone and must release it
        // before the strong count can drop to zero.
        self.stop_scheduler();

        let Some(instance) = self.take_instance() else {
            return;
        };

        if let Err(e) = instance.disable() {
            self.log(LogLevel::Error, format!("disable() failed: {e:#}"));
        }
        instance.clear_settings();
        instance.clear_sql_storage();
        instance.clear_clipboard_writer();
    }

    fn setting_changed(&self, key: &str, value: serde_json::Value) {
        // The host's CoalescingDispatcher (ADR 0026) has
        // already deduplicated rapid same-key writes by the
        // time we get here. Re-encode the JSON value as a
        // string for the WIT crossing — same encoding as
        // `settings::get`.
        let json = match serde_json::to_string(&value) {
            Ok(s) => s,
            Err(e) => {
                self.log(
                    LogLevel::Error,
                    format!("serialize setting value for `{key}`: {e}"),
                );
                return;
            }
        };
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled(&format!("setting_changed({key})"));
            return;
        };
        if let Err(e) = instance.on_setting_changed(key, &json) {
            self.log(
                LogLevel::Error,
                format!("on_setting_changed({key}) failed: {e:#}"),
            );
        }
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled("entries()");
            return vec![];
        };
        match instance.entries() {
            Ok(entries) => entries,
            Err(e) => {
                self.log(LogLevel::Error, format!("entries() failed: {e:#}"));
                vec![]
            }
        }
    }

    fn execute(
        &self,
        entry_id: &str,
        action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled("execute()");
            anyhow::bail!("execute() called on disabled plugin");
        };
        instance.execute(entry_id, action_id)
    }

    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<PluginResponse> {
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled("search()");
            return None;
        };
        match instance.search(query, matched_prefix) {
            Ok(response) => match response {
                PluginResponse::Results(ref entries) if entries.is_empty() => None,
                _ => Some(response),
            },
            Err(e) => {
                self.log(LogLevel::Error, format!("search() failed: {e:#}"));
                None
            }
        }
    }

    fn search_prefixes(&self) -> &[String] {
        &self.manifest.plugin.prefixes
    }

    /// Forward custom frontend messages to the WASM guest's
    /// `messaging::handle-message` export.
    ///
    /// The streaming `_channel` parameter is intentionally
    /// ignored — WASM plugins are strictly request/response.
    /// Plugins that need streaming should stay native, or
    /// wait for a future `messaging-stream` sub-interface.
    ///
    /// Errors are wrapped with explicit prefixes so log
    /// readers can distinguish bridge-level failures
    /// (linker, serialization, store lock) from
    /// plugin-reported failures (the inner `err(string)`
    /// arm of the WIT `result`).
    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled(&format!("handle_message({method})"));
            anyhow::bail!("handle_message() called on disabled plugin");
        };

        // Re-encode the payload as a JSON string for the WIT
        // crossing — same convention as `settings::get`.
        let payload_json =
            serde_json::to_string(&payload).context("serialize handle_message payload")?;

        // Two layers of error: the outer `Result` is the
        // wasmtime / store-lock layer; the inner
        // `Result<String, String>` is what the plugin
        // returned. Plugin-reported errors get a
        // `"plugin error: "` prefix to disambiguate them
        // from bridge failures in logs.
        let result_json = instance
            .handle_message(method, &payload_json)
            .context("invoke guest handle-message")?
            .map_err(|e| anyhow::anyhow!("plugin error: {e}"))?;

        serde_json::from_str(&result_json).context("parse guest handle-message response")
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
impl WasmPluginBridge {
    /// Whether the instance slot is currently populated.
    /// Tests use this to observe lifecycle transitions
    /// without unlocking the slot manually.
    pub(crate) fn instance_is_some(&self) -> bool {
        self.instance
            .lock()
            .expect("instance slot not poisoned")
            .is_some()
    }

    /// Weak clone of the current instance for drop
    /// verification: tests hold the returned `Weak`, run
    /// the tear-down path, and assert that `upgrade()`
    /// returns `None`.
    pub(crate) fn instance_weak(&self) -> Option<std::sync::Weak<WasmPluginInstance>> {
        self.instance
            .lock()
            .expect("instance slot not poisoned")
            .as_ref()
            .map(Arc::downgrade)
    }

    /// Expose the cached `SqlConfig` for test assertions.
    pub(crate) fn sql_config_for_tests(&self) -> &SqlConfig {
        &self.sql_config
    }
}

#[cfg(test)]
mod tests {
    //! Bridge-level tests that exercise the
    //! `ensure_instance`/`take_instance` lifecycle and the
    //! constructor's fail-fast behavior.
    //!
    //! **Scope**: these tests deliberately do *not* go
    //! through the full `Plugin::enable` path. That path
    //! takes a `PluginContext` whose `PluginSettings`
    //! requires a real Tauri store, and there is currently
    //! no lightweight way to build one from a unit test.
    //! Instead, the tests drive the primitives
    //! (`ensure_instance`, guest `enable()`,
    //! `take_instance`) directly — the Plugin trait glue
    //! that composes them is straightforward and covered
    //! by code review.

    use super::*;
    use crate::wasm::logging::channel::LogSender;
    use crate::wasm::logging::spans::SpanRegistry;
    use crate::wasm::runtime::WasmRuntime;
    use crate::wasm::source::DirectorySource;

    const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    fn test_runtime() -> Arc<WasmRuntime> {
        WasmRuntime::new(LogSender::test_sender(), Arc::new(SpanRegistry::new()))
            .expect("runtime construction succeeds")
    }

    /// Build a bridge from a committed fixture directory.
    /// Uses a fresh tempdir for `app_data_dir` so SQL
    /// storage can open without clobbering real files.
    fn test_bridge(
        fixture: &str,
        app_data_dir: &std::path::Path,
    ) -> anyhow::Result<WasmPluginBridge> {
        let fixture_path = std::path::Path::new(FIXTURE_ROOT).join(fixture);
        let source = DirectorySource::open(&fixture_path)
            .with_context(|| format!("open fixture `{fixture}`"))?;
        let manifest = source.manifest().clone();
        WasmPluginBridge::new(
            manifest,
            test_runtime(),
            LogSender::test_sender(),
            &source,
            app_data_dir,
        )
    }

    #[test]
    fn new_compiles_but_does_not_instantiate() {
        // The constructor compiles the component into the
        // runtime cache and materializes the sql config,
        // but the instance slot stays `None` until
        // `ensure_instance` is called.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-plugin", tmp.path()).expect("bridge construction");
        assert!(
            !bridge.instance_is_some(),
            "fresh bridge should have an empty instance slot"
        );
        assert!(matches!(bridge.sql_config_for_tests(), SqlConfig::None));
    }

    #[test]
    fn new_surfaces_compile_errors() {
        // Pointing the bridge at a fixture whose `wasm`
        // field references a file with garbage bytes
        // should fail at the `runtime.compile` step and
        // return an `Err` from the constructor.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bad_plugin_dir = tmp.path().join("bad-plugin");
        std::fs::create_dir_all(&bad_plugin_dir).expect("mkdir");
        std::fs::write(
            bad_plugin_dir.join("manifest.toml"),
            r#"
[plugin]
id = "bad-plugin"
name = "Bad Plugin"
description = "broken wasm"
version = "0.0.0"
wasm = "bad.wasm"
icon = "heroicons:x-mark"
"#,
        )
        .expect("write manifest");
        std::fs::write(bad_plugin_dir.join("bad.wasm"), b"not a wasm file at all")
            .expect("write bad wasm");

        let source = DirectorySource::open(&bad_plugin_dir).expect("open directory");
        let manifest = source.manifest().clone();
        let app_data = tempfile::tempdir().expect("tempdir");

        let result = WasmPluginBridge::new(
            manifest,
            test_runtime(),
            LogSender::test_sender(),
            &source,
            app_data.path(),
        );
        assert!(
            result.is_err(),
            "bridge construction should fail for broken wasm bytes"
        );
    }

    #[test]
    fn new_surfaces_missing_migration_file() {
        // A manifest that declares a migration file which
        // does not exist in the source must fail bridge
        // construction, not silently skip the migration.
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugin_dir = tmp.path().join("sql-plugin");
        std::fs::create_dir_all(&plugin_dir).expect("mkdir");

        // Copy the minimal-plugin wasm in so the compile
        // step succeeds; the failure we want is the
        // migration read.
        let wasm_src = std::path::Path::new(FIXTURE_ROOT).join("minimal-plugin/minimal_plugin.wasm");
        std::fs::copy(&wasm_src, plugin_dir.join("minimal_plugin.wasm")).expect("copy wasm");

        std::fs::write(
            plugin_dir.join("manifest.toml"),
            r#"
[plugin]
id = "sql-plugin"
name = "SQL Plugin"
description = "missing migration"
version = "0.0.0"
wasm = "minimal_plugin.wasm"
icon = "heroicons:circle-stack"

[storage.sql]
migrations = ["migrations/001_init.sql"]
"#,
        )
        .expect("write manifest");

        let source = DirectorySource::open(&plugin_dir).expect("open directory");
        let manifest = source.manifest().clone();
        let app_data = tempfile::tempdir().expect("tempdir");

        let result = WasmPluginBridge::new(
            manifest,
            test_runtime(),
            LogSender::test_sender(),
            &source,
            app_data.path(),
        );
        let err = result.err().expect("missing migration file must error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("001_init.sql"),
            "error should name the missing migration (got: {msg})"
        );
    }

    #[test]
    fn ensure_instance_creates_and_is_idempotent() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-plugin", tmp.path()).expect("bridge construction");

        let first = bridge.ensure_instance().expect("first ensure creates instance");
        assert!(bridge.instance_is_some());

        let second = bridge
            .ensure_instance()
            .expect("second ensure returns the same instance");
        assert!(
            Arc::ptr_eq(&first, &second),
            "repeated ensure_instance should return the same Arc"
        );
    }

    #[test]
    fn take_instance_clears_slot() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-plugin", tmp.path()).expect("bridge construction");

        assert!(bridge.take_instance().is_none(), "empty slot takes nothing");

        bridge.ensure_instance().expect("create instance");
        assert!(bridge.instance_is_some());

        let taken = bridge.take_instance();
        assert!(taken.is_some(), "populated slot returns the Arc");
        assert!(!bridge.instance_is_some(), "take empties the slot");
    }

    #[test]
    fn re_instantiate_after_take_produces_new_instance() {
        // Destroy/recreate: take the current instance,
        // drop it, then ensure again. The second
        // `ensure_instance` must allocate a fresh
        // `WasmPluginInstance` with a distinct `Arc`.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-plugin", tmp.path()).expect("bridge construction");

        let first = bridge.ensure_instance().expect("first ensure");
        let first_ptr = Arc::as_ptr(&first);
        drop(first);
        let _ = bridge.take_instance();
        assert!(!bridge.instance_is_some());

        let second = bridge.ensure_instance().expect("second ensure");
        let second_ptr = Arc::as_ptr(&second);
        assert_ne!(
            first_ptr, second_ptr,
            "re-instantiation should produce a distinct Arc"
        );
    }

    #[test]
    fn guest_enable_failure_reports_err() {
        // Drive the failing-enable fixture directly: the
        // guest's `enable()` panics, which surfaces as an
        // `Err` from `WasmPluginInstance::enable`. The
        // bridge's `Plugin::enable` uses this signal to
        // tear the slot down.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("failing-enable-plugin", tmp.path())
            .expect("failing-enable bridge construction");

        let instance = bridge.ensure_instance().expect("instantiate fixture");
        assert!(
            instance.enable().is_err(),
            "failing-enable fixture's guest enable() must return Err"
        );

        // Simulate the bridge's drop-on-failure path:
        // drop our local clone, take the slot.
        drop(instance);
        let taken = bridge.take_instance();
        assert!(taken.is_some(), "take returns the Arc to drop");
        drop(taken);
        assert!(
            !bridge.instance_is_some(),
            "after tear-down the slot is empty"
        );
    }

    #[test]
    fn instance_drop_releases_memory() {
        // After `take_instance` returns and every clone is
        // dropped, the `Weak` must fail to upgrade — proof
        // that no leaked clones are keeping the wasmtime
        // `Store` alive.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-plugin", tmp.path()).expect("bridge construction");

        let instance = bridge.ensure_instance().expect("instantiate");
        let weak = bridge.instance_weak().expect("weak snapshot available");
        assert!(
            weak.upgrade().is_some(),
            "weak upgrades while the instance is alive"
        );

        drop(instance);
        let taken = bridge.take_instance().expect("take the slot");
        drop(taken);

        assert!(
            weak.upgrade().is_none(),
            "weak should not upgrade after the last strong clone is dropped"
        );
    }

    #[test]
    fn sql_config_applied_on_each_instance() {
        // Construct a bridge whose manifest declares a SQL
        // migration. The cached `sql_config` should be
        // `Configured`, and each fresh instance should be
        // able to open its SQL storage — which is a direct
        // check that `ensure_instance` re-applied the
        // config onto the new `PluginState`.
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugin_dir = tmp.path().join("sql-plugin");
        std::fs::create_dir_all(plugin_dir.join("migrations")).expect("mkdir");

        let wasm_src = std::path::Path::new(FIXTURE_ROOT).join("minimal-plugin/minimal_plugin.wasm");
        std::fs::copy(&wasm_src, plugin_dir.join("minimal_plugin.wasm")).expect("copy wasm");

        std::fs::write(
            plugin_dir.join("migrations/001_init.sql"),
            "CREATE TABLE IF NOT EXISTS probe (id INTEGER PRIMARY KEY);",
        )
        .expect("write migration");
        std::fs::write(
            plugin_dir.join("manifest.toml"),
            r#"
[plugin]
id = "sql-plugin"
name = "SQL Plugin"
description = "plugin with sql config"
version = "0.0.0"
wasm = "minimal_plugin.wasm"
icon = "heroicons:circle-stack"

[storage.sql]
migrations = ["migrations/001_init.sql"]
"#,
        )
        .expect("write manifest");

        let app_data = tempfile::tempdir().expect("app data tempdir");
        let source = DirectorySource::open(&plugin_dir).expect("open directory");
        let manifest = source.manifest().clone();
        let bridge = WasmPluginBridge::new(
            manifest,
            test_runtime(),
            LogSender::test_sender(),
            &source,
            app_data.path(),
        )
        .expect("bridge construction");

        assert!(
            matches!(bridge.sql_config_for_tests(), SqlConfig::Configured { .. }),
            "bridge should cache Configured sql config"
        );

        // Enable → disable → enable drives two
        // instantiations. Both must be able to open SQL
        // storage, which exercises the re-application path.
        let first = bridge.ensure_instance().expect("first ensure");
        first
            .open_sql_storage()
            .expect("first instance opens SQL storage");
        drop(first);
        let _ = bridge.take_instance();

        let second = bridge.ensure_instance().expect("second ensure");
        second
            .open_sql_storage()
            .expect("second instance re-applies sql config and opens storage");
    }

    #[test]
    fn manifest_without_tasks_parses_no_scheduler_entries() {
        // The minimal fixture declares no `[[tasks]]`, so
        // `parsed_tasks` should end up empty and
        // `spawn_scheduler` will become a no-op.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-plugin", tmp.path()).expect("bridge construction");
        assert!(
            bridge.parsed_tasks.is_empty(),
            "fixture has no tasks, parsed list should be empty"
        );
    }

    #[test]
    fn parses_task_schedules_when_present() {
        // Verify the bridge correctly ingests `[[tasks]]`
        // entries into `parsed_tasks`. Uses a tempdir
        // manifest because no committed fixture has
        // tasks declared.
        let tmp = tempfile::tempdir().expect("tempdir");
        let plugin_dir = tmp.path().join("task-plugin");
        std::fs::create_dir_all(&plugin_dir).expect("mkdir");

        let wasm_src = std::path::Path::new(FIXTURE_ROOT).join("minimal-plugin/minimal_plugin.wasm");
        std::fs::copy(&wasm_src, plugin_dir.join("minimal_plugin.wasm")).expect("copy wasm");

        std::fs::write(
            plugin_dir.join("manifest.toml"),
            r#"
[plugin]
id = "task-plugin"
name = "Task Plugin"
description = "plugin with scheduled tasks"
version = "0.0.0"
wasm = "minimal_plugin.wasm"
icon = "heroicons:clock"

[[tasks]]
id = "hourly"
schedule = "0 * * * *"

[[tasks]]
id = "every-five"
schedule = "*/5 * * * *"
"#,
        )
        .expect("write manifest");

        let app_data = tempfile::tempdir().expect("tempdir");
        let source = DirectorySource::open(&plugin_dir).expect("open directory");
        let manifest = source.manifest().clone();
        let bridge = WasmPluginBridge::new(
            manifest,
            test_runtime(),
            LogSender::test_sender(),
            &source,
            app_data.path(),
        )
        .expect("bridge construction");

        assert_eq!(bridge.parsed_tasks.len(), 2);
        assert_eq!(bridge.parsed_tasks[0].id, "hourly");
        assert_eq!(bridge.parsed_tasks[1].id, "every-five");
    }
}
