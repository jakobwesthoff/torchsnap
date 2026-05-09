// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Gadget Bridge
//
// Host-side representation of a WASM gadget. Owns the
// gadget's full lifecycle: compiles the component at
// construction, lazily instantiates a `WasmGadgetInstance`
// on enable, and drops it on disable so the wasmtime
// `Store` and all guest-side linear memory are reclaimed.
// Implements the native `Gadget` trait so WASM gadgets
// participate in the existing `GadgetHost` dispatch
// pipeline alongside native gadgets.
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

use crate::commands::types::{ActionId, CatalogEntry, GadgetResponse, PostAction, ScoredEntry};
use crate::frecency::GadgetFrecency;
use crate::gadgets::{Gadget, ProvisioningContext};
use crate::settings::{GadgetSettings, SettingsInit};

use super::logging::channel::{LogContext, LogSender};
use super::logging::{LogItem, LogItemKind, LogLevel, LogSource};
use super::manifest::Manifest;
use crate::paths::GadgetPaths;
use crate::network::website_metadata::WebsiteMetadataService;

use super::runtime::{
    CachedComponent, SqlConfig, WasmGadgetCaps, WasmGadgetInstance, WasmRuntime,
};
use super::source::GadgetSource;

// =========================================================
// WasmGadgetBridge
// =========================================================

/// Host-side representation of a WASM gadget. Owns the
/// manifest, a handle to the shared `WasmRuntime`, the
/// materialized `SqlConfig`, and the currently live
/// instance slot.
pub struct WasmGadgetBridge {
    manifest: Manifest,
    gadget_id: String,
    /// Disk-cached compiled component with lazy acquire/release.
    /// Owns the `Arc<WasmRuntime>` reference internally.
    cached: Mutex<CachedComponent>,
    /// Re-applied to every fresh `WasmGadgetInstance` on
    /// enable — each new `GadgetState` starts with
    /// `SqlConfig::None`, so the bridge holds the
    /// materialized config here to survive disable/re-enable
    /// cycles without re-reading the gadget source.
    sql_config: SqlConfig,
    /// Retained so `enable()` can stash it on `GadgetState`
    /// for the `assets::read` / `assets::exists` host
    /// imports, which call into the source at runtime
    /// (during guest `enable`, search, messaging) long after
    /// bridge construction.
    ///
    /// Stored as `Arc<_>` because the same underlying source
    /// is simultaneously retained by `wasm::protocol`'s
    /// `GadgetSourceRegistry` for frontend asset serving
    /// (the `torchsnap-gadget://` custom protocol handler).
    /// Two independent owners of the same source — shared
    /// ownership = `Arc`. `Box` would force a double-open
    /// (re-mmap'ing the zip for `ArchiveSource`) or break
    /// the registry side of the contract.
    ///
    /// The `+ Send + Sync` bound is required by the dyn
    /// type signature because the bridge crosses threads
    /// via the scheduler tokio tasks. The `GadgetSource`
    /// trait itself is already declared
    /// `pub trait GadgetSource: Send + Sync`, so every impl
    /// already qualifies — the bound here is purely a
    /// compile-time assertion.
    gadget_source: Arc<dyn GadgetSource + Send + Sync>,
    /// Host-built capabilities received at construction.
    caps: Arc<crate::caps::ProvisionedCaps>,
    /// Platform paths resolved once at construction, used by the
    /// transitional WasmGadgetCaps adapter in enable() to build
    /// GadgetPaths. Goes away when WasmGadgetCaps is deleted.
    platform_paths: Arc<crate::paths::PlatformPaths>,
    /// Resolved `${gadget-data}` for this gadget —
    /// `<app_data_dir>/gadget-home/<gadget-id>/`. Re-stashed
    /// on every fresh instance so per-call `paths::resolve`
    /// substitutions go through one source of truth.
    gadget_data: PathBuf,
    /// Resolved `${gadget-archive}` for this gadget — the
    /// directory root for `DirectorySource`, or the
    /// `.torchsnap` archive file for `ArchiveSource`. See the
    /// `GadgetSource::root_path` docs for the per-source
    /// contract.
    gadget_archive: PathBuf,
    /// Live guest instance, or `None` while disabled.
    ///
    /// **Lock discipline**: never call into the guest while
    /// holding this lock. Every access is
    /// `lock → Arc::clone → drop lock → call`, so the outer
    /// lock only covers slot creation / teardown. The
    /// `WasmGadgetInstance`'s inner store mutex is what
    /// serializes guest calls.
    instance: Mutex<Option<Arc<WasmGadgetInstance>>>,
    log_sender: LogSender,
    /// Pre-parsed `[[tasks]]` entries from the manifest. The
    /// raw schedule strings are validated at manifest load
    /// time; this list holds the parsed `cron::Schedule`s
    /// alongside their ids ready to be consumed by the
    /// scheduler loop in `enable()`.
    parsed_tasks: Vec<ParsedTask>,
    /// Tokio handle for the running scheduler loop. `None`
    /// while the gadget is disabled or when the gadget has
    /// no `[[tasks]]` declared. The `Mutex` covers the
    /// `enable()` / `disable()` swap, not the loop itself.
    scheduler_handle: Mutex<Option<JoinHandle<()>>>,
    /// Co-operative shutdown flag the scheduler loop
    /// checks before each `run_task` invocation.
    /// `tokio::JoinHandle::abort()` only takes effect at
    /// the next `await` point — for the scheduler that
    /// is the next `tokio::time::sleep`. Without this
    /// flag, an in-flight `run_task` would still finish
    /// against a half-disabled gadget between
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

impl WasmGadgetBridge {
    /// Build a `Vec<CapRequest>` from a parsed manifest and its
    /// gadget source. Reads migration SQL files from the source
    /// when `[storage.sql]` is declared.
    ///
    /// This is an inherent associated function (not on the
    /// `Gadget` trait) because WASM cap requests are
    /// instance-dependent (derived from the manifest), not
    /// statically known from the type.
    pub fn cap_requests_from_manifest(
        manifest: &Manifest,
        source: &dyn GadgetSource,
    ) -> anyhow::Result<Vec<crate::caps::CapRequest>> {
        use anyhow::Context;
        use crate::caps::CapRequest;

        let mut requests = Vec::new();

        // Always-on caps for WASM gadgets.
        requests.push(CapRequest::Settings);
        requests.push(CapRequest::Frecency);
        requests.push(CapRequest::Clipboard);
        requests.push(CapRequest::PathResolver);

        if let Some(perms) = &manifest.permissions {
            if let Some(opener) = &perms.opener {
                requests.push(opener.clone().into());
            }
            if let Some(http) = &perms.http {
                requests.push(http.clone().into());
            }
            if let Some(fs) = &perms.fs {
                requests.push(fs.clone().into());
            }
            if !perms.command.is_empty() {
                requests.push(perms.command.clone().into());
            }
            if perms.website_metadata {
                requests.push(CapRequest::WebsiteMetadata);
            }
        }

        // SqlStorage: read migration file contents from the source.
        if let Some(sql_def) = manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
            let mut migration_contents = Vec::with_capacity(sql_def.migrations.len());
            for path in &sql_def.migrations {
                let bytes = source
                    .read_file(path)
                    .with_context(|| format!("read SQL migration file `{path}`"))?;
                let text = String::from_utf8(bytes).with_context(|| {
                    format!("SQL migration file `{path}` is not valid UTF-8")
                })?;
                migration_contents.push(text);
            }
            requests.push(CapRequest::SqlStorage {
                config: crate::caps::SqlStorageConfig {
                    migrations: migration_contents,
                },
            });
        }

        Ok(requests)
    }

    /// Build a bridge from a parsed manifest, a handle to
    /// the shared runtime, and the gadget source.
    ///
    /// The bridge receives `Arc<ProvisionedCaps>` at construction
    /// — the host built them from the `cap_requests_from_manifest`
    /// output.
    ///
    /// Construction is cheap — no WASM compilation happens
    /// here. The `CachedComponent` compiles (or deserializes
    /// from the disk cache) on first `acquire()`, which is
    /// triggered by `ensure_instance()` during `enable()`.
    pub fn new(
        manifest: Manifest,
        runtime: Arc<WasmRuntime>,
        log_ctx: LogContext,
        source: Arc<dyn GadgetSource + Send + Sync>,
        app_data_dir: &std::path::Path,
        caps: Arc<crate::caps::ProvisionedCaps>,
        platform_paths: Arc<crate::paths::PlatformPaths>,
    ) -> anyhow::Result<Self> {
        use anyhow::Context;

        let log_sender = log_ctx.sender.clone();
        let gadget_id = manifest.gadget.id.as_str().to_string();

        // Pre-resolve gadget paths for the WasmGadgetCaps adapter
        // in enable(). These are also stored on the bridge for
        // the cached component's disk cache path.
        let gadget_data = app_data_dir.join("gadget-home").join(gadget_id.as_str());
        let gadget_archive = source.root_path().to_path_buf();

        // Pre-parse every `[[tasks]]` schedule.
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

        // SqlConfig is still needed by the bridge for the
        // WasmGadgetCaps adapter in enable() — it's used to
        // check whether sql_handle_reps teardown is needed.
        // TODO: Remove once WasmGadgetCaps is deleted.
        let sql_config = match manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
            None => SqlConfig::None,
            Some(_) => SqlConfig::Configured {
                db_path: gadget_data.join("sql").join("storage.sqlite3"),
                migrations: Arc::new(vec![]),
            },
        };

        let cached = CachedComponent::new(
            runtime,
            log_ctx,
            Arc::clone(&source),
            gadget_data.clone(),
        );

        Ok(Self {
            manifest,
            gadget_id,
            cached: Mutex::new(cached),
            sql_config,
            gadget_source: source,
            caps,
            platform_paths,
            gadget_data,
            gadget_archive,
            instance: Mutex::new(None),
            log_sender,
            parsed_tasks,
            scheduler_handle: Mutex::new(None),
            scheduler_shutdown: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Return a clone of the live instance, creating one
    /// via `runtime.instantiate` if the slot is empty.
    fn ensure_instance(&self) -> anyhow::Result<Arc<WasmGadgetInstance>> {
        let mut slot = self.instance.lock().expect("instance slot not poisoned");
        if let Some(existing) = slot.as_ref() {
            return Ok(Arc::clone(existing));
        }

        let instance = self
            .cached
            .lock()
            .expect("cached component not poisoned")
            .instantiate()
            .with_context(|| format!("instantiate WASM gadget `{}`", self.gadget_id))?;

        let arc = Arc::new(instance);
        *slot = Some(Arc::clone(&arc));
        Ok(arc)
    }

    /// Take the instance out of the slot. The caller must
    /// drop every other `Arc` clone (notably the scheduler
    /// task's) before the underlying `Store` can be freed.
    fn take_instance(&self) -> Option<Arc<WasmGadgetInstance>> {
        self.instance
            .lock()
            .expect("instance slot not poisoned")
            .take()
    }

    /// Spawn the per-gadget scheduler tokio task that walks
    /// every `[[tasks]]` entry, sleeps until the earliest
    /// next fire across all of them, and invokes the WIT
    /// `tasks::run-task` guest export.
    ///
    /// The `instance` argument is passed in by
    /// `Gadget::enable` from the already-cloned `Arc` it
    /// holds — the scheduler task needs its own clone so
    /// it can keep the instance alive for as long as the
    /// task is running, independent of the bridge's
    /// instance slot.
    ///
    /// Gadgets without `[[tasks]]` get nothing — no tokio
    /// task is spawned at all, so the cost is zero for
    /// gadgets that don't use the API.
    fn spawn_scheduler(&self, instance: Arc<WasmGadgetInstance>) {
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
        // the shutdown flag, and a gadget id for log
        // tagging. The `instance` clone is already held by
        // the caller and handed to us by value.
        let shutdown = Arc::clone(&self.scheduler_shutdown);
        let log_sender = self.log_sender.clone();
        let gadget_id = self.gadget_id.clone();
        let schedules: Vec<(String, Schedule)> = self
            .parsed_tasks
            .iter()
            .map(|t| (t.id.clone(), t.schedule.clone()))
            .collect();

        let handle = tauri::async_runtime::spawn(async move {
            scheduler_loop(instance, schedules, shutdown, log_sender, gadget_id).await;
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
// Per-gadget scheduler loop
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
    instance: Arc<WasmGadgetInstance>,
    schedules: Vec<(String, Schedule)>,
    shutdown: Arc<AtomicBool>,
    log_sender: LogSender,
    gadget_id: String,
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
            // sub-millisecond time). If a gadget author writes a
            // pathological task — e.g. a multi-second web scrape or a
            // large file scan — it will block one tokio worker
            // thread for the duration. With the default Tauri
            // runtime (4 worker threads), four such gadgets
            // scheduling simultaneously would saturate the runtime.
            // The fix when this happens is to wrap `instance.run_task`
            // in `tokio::task::spawn_blocking(...).await?`; nothing
            // about the guest interface needs to change.
            match instance.run_task(id) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log_task_error(&log_sender, &gadget_id, id, &e),
                Err(e) => log_task_error(&log_sender, &gadget_id, id, &format!("{e:#}")),
            }
        }
    }
}

/// Build a [`GadgetPaths`] resolving the five `${...}`
/// substitution variables (`gadget-data`, `gadget-archive`,
/// `home`, `xdg-config`, `xdg-data`) for one gadget
/// instance. Called from `enable()` once per re-enable
/// cycle. The gadget-data and gadget-archive paths come
/// from the bridge (the bridge already has them); the
/// XDG-style paths come from Tauri's path resolver, which
/// produces platform-correct values (`Application Support`
/// on macOS, `%APPDATA%` on Windows, `$XDG_CONFIG_HOME`
/// with fallback on Linux).
///
/// TODO: Platform paths should be resolved once at startup
/// and stored on GadgetHost as `Arc<PlatformPaths>`. This
/// function would then only assemble the per-gadget portion.
fn build_gadget_paths(
    app: &tauri::AppHandle,
    gadget_data: &PathBuf,
    gadget_archive: &PathBuf,
) -> anyhow::Result<GadgetPaths> {
    use tauri::Manager;

    let path_resolver = app.path();
    let home = path_resolver.home_dir().context("resolve home directory")?;
    let xdg_config = path_resolver
        .config_dir()
        .context("resolve config directory")?;
    let xdg_data = path_resolver.data_dir().context("resolve data directory")?;

    Ok(GadgetPaths {
        platform: std::sync::Arc::new(crate::paths::PlatformPaths {
            home,
            xdg_config,
            xdg_data,
        }),
        gadget_data: gadget_data.clone(),
        gadget_archive: gadget_archive.clone(),
    })
}

fn log_task_error(log_sender: &LogSender, gadget_id: &str, task_id: &str, error: &str) {
    log_sender.send(LogItem {
        seq: 0,
        timestamp: SystemTime::now(),
        source: LogSource::Gadget(gadget_id.to_string()),
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

impl WasmGadgetBridge {
    /// Emit a log entry for bridge-level events (errors from
    /// guest calls that are caught and handled here).
    fn log(&self, level: LogLevel, message: String) {
        self.log_sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: LogSource::Gadget(self.gadget_id.clone()),
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
    /// `Gadget::enable → Result` follow-up todo).
    fn current_instance(&self) -> Option<Arc<WasmGadgetInstance>> {
        self.instance
            .lock()
            .expect("instance slot not poisoned")
            .as_ref()
            .map(Arc::clone)
    }

    /// Log a "dispatched into disabled gadget" event with a
    /// uniform `bug:` prefix. The only path that reaches
    /// this today is the known transient window after a
    /// guest `enable()` failure; once the `Gadget::enable →
    /// Result` follow-up closes that window the branch is
    /// dead and the call sites should be upgraded to
    /// `debug_assert!` / `unreachable!()`.
    fn log_dispatched_while_disabled(&self, method: &str) {
        self.log(
            LogLevel::Error,
            format!("bug: {method} dispatched on disabled gadget"),
        );
    }
}

impl Gadget for WasmGadgetBridge {
    fn id(&self) -> &str {
        self.manifest.gadget.id.as_str()
    }

    fn initialize_settings(&self, mut settings: SettingsInit) -> SettingsInit {
        for (key, toml_value) in &self.manifest.settings {
            if let Ok(json_value) = serde_json::to_value(toml_value) {
                settings = settings.ensure(key, json_value);
            }
        }
        settings
    }

    fn enable(&self) {
        let instance = match self.ensure_instance() {
            Ok(instance) => instance,
            Err(e) => {
                self.log(
                    LogLevel::Error,
                    format!("failed to instantiate gadget: {e:#}"),
                );
                return;
            }
        };

        // Transitional adapter: build WasmGadgetCaps from the
        // host-provisioned ProvisionedCaps. This adapter goes away
        // when WasmGadgetCaps is merged into ProvisionedCaps
        // (Phase D of the provisioning restructuring).
        let gadget_paths = GadgetPaths {
            platform: Arc::clone(&self.platform_paths),
            gadget_data: self.gadget_data.clone(),
            gadget_archive: self.gadget_archive.clone(),
        };

        let wasm_caps = WasmGadgetCaps {
            settings: self.caps.settings.clone(),
            frecency: self.caps.frecency.clone(),
            gadget_paths,
            sql_storage: self.caps.sql_storage.clone(),
            clipboard: self.caps.clipboard().clone(),
            opener: self.caps.opener().clone(),
            http: self.caps.http().clone(),
            filesystem: self.caps.filesystem.clone(),
            command: self.caps.command.clone(),
            website_metadata: self.caps.website_metadata.clone(),
        };

        instance.set_gadget_source(Arc::clone(&self.gadget_source));
        instance.set_caps(wasm_caps);

        if let Err(e) = instance.enable() {
            self.log(LogLevel::Error, format!("guest enable() failed: {e:#}"));
            instance.clear_caps();
            drop(instance);
            let _ = self.take_instance();
            return;
        }

        self.spawn_scheduler(instance);
    }

    fn disable(&self) {
        self.stop_scheduler();

        let Some(instance) = self.take_instance() else {
            return;
        };

        if let Err(e) = instance.disable() {
            self.log(LogLevel::Error, format!("disable() failed: {e:#}"));
        }

        instance.clear_caps();

        self.cached
            .lock()
            .expect("cached component not poisoned")
            .release();
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
            Ok(mut entries) => {
                let warnings = super::bindings::resolve_catalog_entries_asset_icons(
                    &mut entries,
                    &self.gadget_id,
                );
                for w in warnings {
                    self.log(LogLevel::Warn, w);
                }
                entries
            }
            Err(e) => {
                self.log(LogLevel::Error, format!("entries() failed: {e:#}"));
                vec![]
            }
        }
    }

    fn execute(
        &self,
        entry: &ScoredEntry,
        action_id: &ActionId,
    ) -> anyhow::Result<PostAction> {
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled("execute()");
            anyhow::bail!("execute() called on disabled gadget");
        };
        instance.execute(entry, action_id)
    }

    fn search(&self, query: &str, matched_prefix: Option<&str>) -> Option<GadgetResponse> {
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled("search()");
            return None;
        };
        match instance.search(query, matched_prefix) {
            // `None` is reserved for "gadget not participating"
            // (disabled / errored); an empty Results must pass
            // through so the host can forward it and evict stale
            // per-source entries on the frontend.
            Ok(mut response) => {
                let warnings = super::bindings::resolve_search_response_asset_icons(
                    &mut response,
                    &self.gadget_id,
                );
                for w in warnings {
                    self.log(LogLevel::Warn, w);
                }
                Some(response)
            }
            Err(e) => {
                self.log(LogLevel::Error, format!("search() failed: {e:#}"));
                None
            }
        }
    }

    fn search_prefixes(&self) -> &[String] {
        &self.manifest.gadget.prefixes
    }

    /// Forward custom frontend messages to the WASM guest's
    /// `messaging::handle-message` export.
    ///
    /// The streaming `_channel` parameter is intentionally
    /// ignored — WASM gadgets are strictly request/response.
    /// Gadgets that need streaming should stay native, or
    /// wait for a future `messaging-stream` sub-interface.
    ///
    /// Errors are wrapped with explicit prefixes so log
    /// readers can distinguish bridge-level failures
    /// (linker, serialization, store lock) from
    /// gadget-reported failures (the inner `err(string)`
    /// arm of the WIT `result`).
    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        let Some(instance) = self.current_instance() else {
            self.log_dispatched_while_disabled(&format!("handle_message({method})"));
            anyhow::bail!("handle_message() called on disabled gadget");
        };

        // Re-encode the payload as a JSON string for the WIT
        // crossing — same convention as `settings::get`.
        let payload_json =
            serde_json::to_string(&payload).context("serialize handle_message payload")?;

        // Two layers of error: the outer `Result` is the
        // wasmtime / store-lock layer; the inner
        // `Result<String, String>` is what the gadget
        // returned. Gadget-reported errors get a
        // `"gadget error: "` prefix to disambiguate them
        // from bridge failures in logs.
        let result_json = instance
            .handle_message(method, &payload_json)
            .context("invoke guest handle-message")?
            .map_err(|e| anyhow::anyhow!("gadget error: {e}"))?;

        serde_json::from_str(&result_json).context("parse guest handle-message response")
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
impl WasmGadgetBridge {
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
    pub(crate) fn instance_weak(&self) -> Option<std::sync::Weak<WasmGadgetInstance>> {
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
    //! Bridge-level tests.
    //!
    //! These exercise the `ensure_instance`/`take_instance`
    //! lifecycle and the constructor's fail-fast behavior
    //! directly. The full `Gadget::enable` path is not
    //! covered because `GadgetContext.settings` needs a
    //! real Tauri store; the primitives it composes are
    //! driven directly instead.

    use super::*;
    use crate::wasm::logging::channel::LogContext;
    use crate::wasm::runtime::caps::WasmGadgetCaps;
    use crate::wasm::runtime::WasmRuntime;
    use crate::wasm::source::DirectorySource;

    const FIXTURE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

    fn test_runtime() -> Arc<WasmRuntime> {
        WasmRuntime::new().expect("runtime construction succeeds")
    }

    fn test_platform_paths() -> Arc<crate::paths::PlatformPaths> {
        Arc::new(crate::paths::PlatformPaths {
            home: std::path::PathBuf::from("/home/test"),
            xdg_config: std::path::PathBuf::from("/home/test/.config"),
            xdg_data: std::path::PathBuf::from("/home/test/.local/share"),
        })
    }

    fn test_caps() -> Arc<crate::caps::ProvisionedCaps> {
        Arc::new(test_caps_inner())
    }

    /// Build a bridge from a committed fixture directory.
    /// Uses a fresh tempdir for `app_data_dir` so SQL
    /// storage can open without clobbering real files.
    fn test_bridge(
        fixture: &str,
        app_data_dir: &std::path::Path,
    ) -> anyhow::Result<WasmGadgetBridge> {
        let fixture_path = std::path::Path::new(FIXTURE_ROOT).join(fixture);
        let source: Arc<dyn GadgetSource + Send + Sync> = Arc::new(
            DirectorySource::open(&fixture_path)
                .with_context(|| format!("open fixture `{fixture}`"))?,
        );
        let manifest = source.manifest().clone();
        WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data_dir,
            test_caps(),
            test_platform_paths(),
        )
    }

    /// Build a `WasmGadgetCaps` from a bridge for tests that
    /// verify SQL storage across enable cycles. Reads migration
    /// SQL from the bridge's caps (populated by
    /// cap_requests_from_manifest during test_bridge_with_sql).
    fn build_test_caps(bridge: &WasmGadgetBridge) -> WasmGadgetCaps {
        WasmGadgetCaps {
            sql_storage: bridge.caps.sql_storage.clone(),
            ..WasmGadgetCaps::default_for_test()
        }
    }

    /// Build a bridge from a fixture, reading SQL migrations
    /// and building proper ProvisionedCaps with SqlStorageCap.
    fn test_bridge_with_sql(
        fixture: &str,
        app_data_dir: &std::path::Path,
    ) -> anyhow::Result<WasmGadgetBridge> {
        let fixture_path = std::path::Path::new(FIXTURE_ROOT).join(fixture);
        let source: Arc<dyn GadgetSource + Send + Sync> = Arc::new(
            DirectorySource::open(&fixture_path)
                .with_context(|| format!("open fixture `{fixture}`"))?,
        );
        let manifest = source.manifest().clone();
        let gadget_id = manifest.gadget.id.as_str();

        // Read migration SQL from the source.
        let sql_storage = if let Some(sql_def) = manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
            let mut migration_contents = Vec::new();
            for path in &sql_def.migrations {
                let bytes = source.read_file(path)?;
                migration_contents.push(String::from_utf8(bytes)?);
            }
            let db_path = app_data_dir
                .join("gadget-home")
                .join(gadget_id)
                .join("sql")
                .join("storage.sqlite3");
            let migration_strs: Vec<&str> = migration_contents.iter().map(String::as_str).collect();
            let storage = crate::storage::SqlStorage::open(db_path, &migration_strs)?;
            Some(Arc::new(crate::caps::SqlStorageCap::new(Arc::new(storage))))
        } else {
            None
        };

        let caps = test_caps_with_sql(sql_storage);

        WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data_dir,
            caps,
            test_platform_paths(),
        )
    }

    fn test_caps_with_sql(
        sql_storage: Option<Arc<crate::caps::SqlStorageCap>>,
    ) -> Arc<crate::caps::ProvisionedCaps> {
        Arc::new(crate::caps::ProvisionedCaps {
            sql_storage,
            ..test_caps_inner()
        })
    }

    fn test_caps_inner() -> crate::caps::ProvisionedCaps {
        crate::caps::ProvisionedCaps {
            opener: Some(Arc::new(crate::caps::OpenerCap::from_closures(
                crate::caps::OpenerPermissions {
                    schemes: vec![],
                    open_path: false,
                    reveal_path: false,
                },
                Box::new(|_| Ok(())),
                Box::new(|_| Ok(())),
                Box::new(|_| Ok(())),
            ))),
            http: Some(Arc::new(crate::caps::HttpCap::new(vec![]))),
            filesystem: None,
            command: None,
            clipboard: Some(Arc::new(crate::caps::ClipboardCap::new(Box::new(|_| Ok(()))))),
            sql_storage: None,
            website_metadata: None,
            icon_cache: None,
            settings: None,
            frecency: None,
            path_resolver: None,
        }
    }

    #[test]
    fn new_compiles_but_does_not_instantiate() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-gadget", tmp.path()).expect("bridge construction");
        assert!(!bridge.instance_is_some());
        assert!(matches!(bridge.sql_config_for_tests(), SqlConfig::None));
    }

    #[test]
    fn new_surfaces_compile_errors() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bad_gadget_dir = tmp.path().join("bad-gadget");
        std::fs::create_dir_all(&bad_gadget_dir).expect("mkdir");
        std::fs::write(
            bad_gadget_dir.join("manifest.toml"),
            r#"
[gadget]
id = "bad-gadget"
name = "Bad Gadget"
description = "broken wasm"
version = "0.0.0"
wasm = "bad.wasm"
icon = "heroicons:x-mark"
"#,
        )
        .expect("write manifest");
        std::fs::write(bad_gadget_dir.join("bad.wasm"), b"not a wasm file at all")
            .expect("write bad wasm");

        let source: Arc<dyn GadgetSource + Send + Sync> =
            Arc::new(DirectorySource::open(&bad_gadget_dir).expect("open directory"));
        let manifest = source.manifest().clone();
        let app_data = tempfile::tempdir().expect("tempdir");

        let result = WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data.path(),
            test_caps(),
            test_platform_paths(),
        );
        // Construction is lazy — the error surfaces on first
        // acquire/instantiate, not at bridge construction time.
        let bridge = result.expect("lazy construction succeeds");
        assert!(bridge.ensure_instance().is_err());
    }

    #[test]
    fn cap_requests_surfaces_missing_migration_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let gadget_dir = tmp.path().join("sql-gadget");
        std::fs::create_dir_all(&gadget_dir).expect("mkdir");

        // Copy the minimal-gadget wasm so the compile step
        // passes; we want the migration-read step to fail.
        let wasm_src =
            std::path::Path::new(FIXTURE_ROOT).join("minimal-gadget/minimal_gadget.wasm");
        std::fs::copy(&wasm_src, gadget_dir.join("minimal_gadget.wasm")).expect("copy wasm");

        std::fs::write(
            gadget_dir.join("manifest.toml"),
            r#"
[gadget]
id = "sql-gadget"
name = "SQL Gadget"
description = "missing migration"
version = "0.0.0"
wasm = "minimal_gadget.wasm"
icon = "heroicons:circle-stack"

[storage.sql]
migrations = ["migrations/001_init.sql"]
"#,
        )
        .expect("write manifest");

        let source: Arc<dyn GadgetSource + Send + Sync> =
            Arc::new(DirectorySource::open(&gadget_dir).expect("open directory"));
        let manifest = source.manifest().clone();

        let result = WasmGadgetBridge::cap_requests_from_manifest(&manifest, &*source);
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
        let bridge = test_bridge("minimal-gadget", tmp.path()).expect("bridge construction");

        let first = bridge.ensure_instance().expect("first ensure");
        assert!(bridge.instance_is_some());

        let second = bridge.ensure_instance().expect("second ensure");
        assert!(
            Arc::ptr_eq(&first, &second),
            "repeated ensure_instance should return the same Arc"
        );
    }

    #[test]
    fn take_instance_clears_slot() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-gadget", tmp.path()).expect("bridge construction");

        assert!(bridge.take_instance().is_none());

        bridge.ensure_instance().expect("create instance");
        assert!(bridge.instance_is_some());

        assert!(bridge.take_instance().is_some());
        assert!(!bridge.instance_is_some());
    }

    #[test]
    fn re_instantiate_after_take_produces_new_instance() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-gadget", tmp.path()).expect("bridge construction");

        // Keep `first` alive across the comparison. Its `ArcInner`
        // slot stays occupied, so the allocator must hand `second`
        // a different address — making `Arc::ptr_eq` a well-defined
        // identity check. Dropping `first` before re-ensuring would
        // free the slot and let the allocator legally reuse the
        // same address, turning this into an allocator-dependent
        // flake rather than a real invariant.
        let first = bridge.ensure_instance().expect("first ensure");
        let _ = bridge.take_instance();
        let second = bridge.ensure_instance().expect("second ensure");

        assert!(
            !Arc::ptr_eq(&first, &second),
            "re-instantiation should produce a distinct Arc"
        );
    }

    #[test]
    fn guest_enable_failure_reports_err() {
        // The fixture's guest `enable()` panics → wasmtime
        // turns the panic into a trap → host sees `Err`.
        // This is the signal `Gadget::enable` uses to tear
        // the slot back down.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("failing-enable-gadget", tmp.path()).expect("bridge construction");

        let instance = bridge.ensure_instance().expect("instantiate");
        assert!(instance.enable().is_err());

        drop(instance);
        assert!(bridge.take_instance().is_some());
        assert!(!bridge.instance_is_some());
    }

    #[test]
    fn instance_drop_releases_memory() {
        // After `take_instance` and every local clone are
        // dropped, the `Weak` must fail to upgrade — proof
        // that nothing is leaking the wasmtime `Store`.
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-gadget", tmp.path()).expect("bridge construction");

        let instance = bridge.ensure_instance().expect("instantiate");
        let weak = bridge.instance_weak().expect("weak snapshot");
        assert!(weak.upgrade().is_some());

        drop(instance);
        drop(bridge.take_instance().expect("take"));

        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn sql_config_applied_on_each_instance() {
        // Each fresh instance must be able to open its SQL
        // storage — proof that `ensure_instance` re-applied
        // the cached config onto the new `GadgetState`
        // (which would otherwise default to `SqlConfig::None`).
        let tmp = tempfile::tempdir().expect("tempdir");
        let gadget_dir = tmp.path().join("sql-gadget");
        std::fs::create_dir_all(gadget_dir.join("migrations")).expect("mkdir");

        let wasm_src =
            std::path::Path::new(FIXTURE_ROOT).join("minimal-gadget/minimal_gadget.wasm");
        std::fs::copy(&wasm_src, gadget_dir.join("minimal_gadget.wasm")).expect("copy wasm");

        std::fs::write(
            gadget_dir.join("migrations/001_init.sql"),
            "CREATE TABLE IF NOT EXISTS probe (id INTEGER PRIMARY KEY);",
        )
        .expect("write migration");
        std::fs::write(
            gadget_dir.join("manifest.toml"),
            r#"
[gadget]
id = "sql-gadget"
name = "SQL Gadget"
description = "gadget with sql config"
version = "0.0.0"
wasm = "minimal_gadget.wasm"
icon = "heroicons:circle-stack"

[storage.sql]
migrations = ["migrations/001_init.sql"]
"#,
        )
        .expect("write manifest");

        let app_data = tempfile::tempdir().expect("app data tempdir");
        let source: Arc<dyn GadgetSource + Send + Sync> =
            Arc::new(DirectorySource::open(&gadget_dir).expect("open directory"));
        let manifest = source.manifest().clone();
        let gadget_id = manifest.gadget.id.as_str();

        // Build caps with actual SQL storage from the source.
        let mut migration_contents = Vec::new();
        if let Some(sql_def) = manifest.storage.as_ref().and_then(|s| s.sql.as_ref()) {
            for path in &sql_def.migrations {
                let bytes = source.read_file(path).expect("read migration");
                migration_contents.push(String::from_utf8(bytes).expect("utf8"));
            }
        }
        let db_path = app_data
            .path()
            .join("gadget-home")
            .join(gadget_id)
            .join("sql")
            .join("storage.sqlite3");
        let migration_strs: Vec<&str> = migration_contents.iter().map(String::as_str).collect();
        let storage = crate::storage::SqlStorage::open(db_path, &migration_strs).expect("open db");
        let caps = test_caps_with_sql(Some(Arc::new(crate::caps::SqlStorageCap::new(Arc::new(storage)))));

        let bridge = WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data.path(),
            caps,
            test_platform_paths(),
        )
        .expect("bridge construction");

        assert!(matches!(
            bridge.sql_config_for_tests(),
            SqlConfig::Configured { .. }
        ));

        // Two instantiations across an explicit teardown
        // exercise that the bridge retains the SQL config
        // and can build working caps for each.
        let first = bridge.ensure_instance().expect("first ensure");
        let caps = build_test_caps(&bridge);
        assert!(
            caps.sql_storage.is_some(),
            "first caps should have SQL storage"
        );
        first.set_caps(caps);
        first.clear_caps();
        drop(first);
        let _ = bridge.take_instance();

        let second = bridge.ensure_instance().expect("second ensure");
        let caps = build_test_caps(&bridge);
        assert!(
            caps.sql_storage.is_some(),
            "second caps should have SQL storage"
        );
        drop(caps);
        drop(second);
    }

    #[test]
    fn manifest_without_tasks_parses_no_scheduler_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let bridge = test_bridge("minimal-gadget", tmp.path()).expect("bridge construction");
        assert!(bridge.parsed_tasks.is_empty());
    }

    /// Construct a WASM bridge with a `[storage.sql]` block
    /// declared in its manifest. `source_root` is where the
    /// gadget files are written (typically a tempdir owned
    /// by the caller so the on-disk layout can be inspected
    /// before cleanup). The single migration creates a
    /// trivial `probe` table; tests do not care about the
    /// contents, only that the path is correct.
    fn bridge_with_sql(
        gadget_id: &str,
        source_root: &std::path::Path,
        app_data_dir: &std::path::Path,
    ) -> anyhow::Result<WasmGadgetBridge> {
        let gadget_dir = source_root.join(gadget_id);
        std::fs::create_dir_all(gadget_dir.join("migrations")).expect("mkdir");

        let wasm_src =
            std::path::Path::new(FIXTURE_ROOT).join("minimal-gadget/minimal_gadget.wasm");
        std::fs::copy(&wasm_src, gadget_dir.join("minimal_gadget.wasm")).expect("copy wasm");

        std::fs::write(
            gadget_dir.join("migrations/001_init.sql"),
            "CREATE TABLE IF NOT EXISTS probe (id INTEGER PRIMARY KEY);",
        )
        .expect("write migration");
        std::fs::write(
            gadget_dir.join("manifest.toml"),
            format!(
                r#"
[gadget]
id = "{gadget_id}"
name = "SQL Gadget"
description = "gadget with sql config"
version = "0.0.0"
wasm = "minimal_gadget.wasm"
icon = "heroicons:circle-stack"

[storage.sql]
migrations = ["migrations/001_init.sql"]
"#
            ),
        )
        .expect("write manifest");

        let source: Arc<dyn GadgetSource + Send + Sync> =
            Arc::new(DirectorySource::open(&gadget_dir).expect("open directory"));
        let manifest = source.manifest().clone();
        WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data_dir,
            test_caps(),
            test_platform_paths(),
        )
    }

    /// The host-managed storage path must follow the
    /// `gadget-home/<id>/sql/storage.sqlite3` layout.
    /// This is a structural guarantee for both gadget
    /// authors (who reason about where their data lives)
    /// and the uninstall flow (which deletes the
    /// `gadget-home/<id>/` subtree to clean up).
    #[test]
    fn sql_config_uses_gadget_home_layout() {
        let source_root = tempfile::tempdir().expect("source tempdir");
        let app_data = tempfile::tempdir().expect("app data tempdir");
        let bridge =
            bridge_with_sql("layout-gadget", source_root.path(), app_data.path()).expect("bridge");

        match bridge.sql_config_for_tests() {
            SqlConfig::Configured { db_path, .. } => {
                let expected = app_data
                    .path()
                    .join("gadget-home")
                    .join("layout-gadget")
                    .join("sql")
                    .join("storage.sqlite3");
                assert_eq!(db_path, &expected);
            }
            SqlConfig::None => panic!("expected SqlConfig::Configured"),
        }
    }

    /// Gadget IDs that share a textual prefix (e.g. `foo` vs
    /// `foo-bar`) must land in separate directories. A bug
    /// that used the ID as a flat filename prefix instead of
    /// a directory segment would let `foo-bar` stomp on
    /// `foo`'s storage.
    #[test]
    fn sql_config_path_isolated_by_gadget_id() {
        let short_root = tempfile::tempdir().expect("short source tempdir");
        let long_root = tempfile::tempdir().expect("long source tempdir");
        let app_data = tempfile::tempdir().expect("app data tempdir");
        let short =
            bridge_with_sql("foo", short_root.path(), app_data.path()).expect("short bridge");
        let long =
            bridge_with_sql("foo-bar", long_root.path(), app_data.path()).expect("long bridge");

        let (
            SqlConfig::Configured { db_path: short, .. },
            SqlConfig::Configured { db_path: long, .. },
        ) = (short.sql_config_for_tests(), long.sql_config_for_tests())
        else {
            panic!("both bridges must have SqlConfig::Configured");
        };

        assert_ne!(short, long);
        // Directory containment also differs — the `foo`
        // tree must not contain anything from `foo-bar`.
        let short_root = short.parent().and_then(|p| p.parent()).expect("foo root");
        let long_root = long
            .parent()
            .and_then(|p| p.parent())
            .expect("foo-bar root");
        assert_ne!(short_root, long_root);
    }

    /// A gadget with no `[storage.sql]` block must not cause
    /// any `gadget-home/` directory to be created: the bridge
    /// constructor is a no-op for storage in that case.
    #[test]
    fn no_sql_config_creates_no_directory() {
        let app_data = tempfile::tempdir().expect("app data tempdir");
        let _bridge = test_bridge("minimal-gadget", app_data.path()).expect("bridge construction");

        let gadget_home = app_data.path().join("gadget-home");
        assert!(
            !gadget_home.exists(),
            "gadget-home/ must not be created for a gadget without [storage.sql]"
        );
    }

    #[test]
    fn parses_task_schedules_when_present() {
        // Tempdir manifest because no committed fixture
        // declares `[[tasks]]`.
        let tmp = tempfile::tempdir().expect("tempdir");
        let gadget_dir = tmp.path().join("task-gadget");
        std::fs::create_dir_all(&gadget_dir).expect("mkdir");

        let wasm_src =
            std::path::Path::new(FIXTURE_ROOT).join("minimal-gadget/minimal_gadget.wasm");
        std::fs::copy(&wasm_src, gadget_dir.join("minimal_gadget.wasm")).expect("copy wasm");

        std::fs::write(
            gadget_dir.join("manifest.toml"),
            r#"
[gadget]
id = "task-gadget"
name = "Task Gadget"
description = "gadget with scheduled tasks"
version = "0.0.0"
wasm = "minimal_gadget.wasm"
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
        let source: Arc<dyn GadgetSource + Send + Sync> =
            Arc::new(DirectorySource::open(&gadget_dir).expect("open directory"));
        let manifest = source.manifest().clone();
        let bridge = WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data.path(),
            test_caps(),
            test_platform_paths(),
        )
        .expect("bridge construction");

        assert_eq!(bridge.parsed_tasks.len(), 2);
        assert_eq!(bridge.parsed_tasks[0].id, "hourly");
        assert_eq!(bridge.parsed_tasks[1].id, "every-five");
    }
}
