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
use super::permission_vars::PathContext;
use crate::network::website_metadata::WebsiteMetadataService;

use super::runtime::host::clipboard::ClipboardState;
use super::runtime::host::command::CommandState;
use super::runtime::host::fs::FsState;
use super::runtime::host::http::HttpState;
use super::runtime::host::opener::{OpenerState, UrlOpenerFn};
use super::runtime::host::sql::SqlState;
use super::runtime::host::website_metadata::WebsiteMetadataState;
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
    /// Permitted URL schemes for `opener::open-url`. Extracted
    /// from `[permissions.opener].schemes` at construction;
    /// empty means the gadget has no opener access.
    opener_schemes: Vec<String>,
    /// Whether the manifest grants `opener::open-path`.
    opener_open_path: bool,
    /// Whether the manifest grants `opener::reveal-path`.
    opener_reveal_path: bool,
    /// Permitted origins for `http::fetch`. Extracted from
    /// `[permissions.http].origins` at construction; empty
    /// means the gadget has no HTTP access; `"*"` means
    /// trust-all.
    http_origins: Vec<String>,
    /// Raw `[permissions.fs] read = [...]` patterns from the
    /// manifest. `${...}` tokens still in place — the bridge
    /// substitutes against the per-instance `PathContext` and
    /// compiles into a `GlobSet` at `enable()`. `None` when
    /// the manifest has no `[permissions.fs]` section, making
    /// every fs call return `permission-denied`.
    fs_patterns_raw: Option<Vec<String>>,
    /// Whether the manifest grants access to the shared
    /// `website-metadata` host import.
    website_metadata_enabled: bool,
    /// Shared website-metadata service. `None` in tests;
    /// production always supplies a real service.
    metadata_service: Option<Arc<WebsiteMetadataService>>,
    /// Raw `[[permissions.command]]` rules pre-extracted from
    /// the manifest. Compiled against the per-instance
    /// `PathContext` at every `enable()` (variable substitution
    /// can change between enable cycles if the host data dirs
    /// move under the gadget). Empty when the manifest declares
    /// no rules.
    command_rules_raw: Vec<super::manifest::CommandPermissionDef>,
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
    /// Build a bridge from a parsed manifest, a handle to
    /// the shared runtime, and the gadget source.
    ///
    /// `app_data_dir` is the host's per-app data root; the
    /// gadget's state lives at
    /// `<app_data_dir>/gadget-home/<gadget-id>/`.
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
        metadata_service: Option<Arc<WebsiteMetadataService>>,
    ) -> anyhow::Result<Self> {
        // Bridge code that emits log items directly (scheduler
        // task errors, lifecycle messages) only needs the
        // sender half; pull it out once for storage.
        let log_sender = log_ctx.sender.clone();
        let gadget_id = manifest.gadget.id.as_str().to_string();

        // Materialize the SQL configuration from the
        // manifest. Gadgets without `[storage.sql]` get
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
                    .join("gadget-home")
                    .join(gadget_id.as_str())
                    .join("sql")
                    .join("storage.sqlite3");

                SqlConfig::Configured {
                    db_path,
                    migrations: Arc::new(migrations),
                }
            }
        };

        // Extract permission allowlists from the manifest.
        // Each list defaults to empty (deny all) when the
        // corresponding `[permissions.*]` sub-table is absent.
        let opener_def = manifest
            .permissions
            .as_ref()
            .and_then(|p| p.opener.as_ref());
        let opener_schemes = opener_def.map(|o| o.schemes.clone()).unwrap_or_default();
        let opener_open_path = opener_def.map(|o| o.open_path).unwrap_or(false);
        let opener_reveal_path = opener_def.map(|o| o.reveal_path).unwrap_or(false);

        let http_origins = manifest
            .permissions
            .as_ref()
            .and_then(|p| p.http.as_ref())
            .map(|h| h.origins.clone())
            .unwrap_or_default();

        let fs_patterns_raw = manifest
            .permissions
            .as_ref()
            .and_then(|p| p.fs.as_ref())
            .map(|fs| fs.read.clone());

        let website_metadata_enabled = manifest
            .permissions
            .as_ref()
            .map(|p| p.website_metadata)
            .unwrap_or(false);

        let command_rules_raw = manifest
            .permissions
            .as_ref()
            .map(|p| p.command.clone())
            .unwrap_or_default();

        // Pre-resolve the host filesystem paths the
        // `paths::resolve` host import (and, in the next
        // sub-phase, command-rule compilation) will need.
        // Gadget-archive comes from the source's filesystem
        // root; gadget-data is the per-gadget host-managed
        // state directory under `<app_data_dir>/gadget-home/`.
        let gadget_data = app_data_dir.join("gadget-home").join(gadget_id.as_str());
        let gadget_archive = source.root_path().to_path_buf();

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
            opener_schemes,
            opener_open_path,
            opener_reveal_path,
            http_origins,
            fs_patterns_raw,
            website_metadata_enabled,
            metadata_service,
            command_rules_raw,
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

/// Build a [`PathContext`] resolving the five `${...}`
/// substitution variables (`gadget-data`, `gadget-archive`,
/// `home`, `xdg-config`, `xdg-data`) for one gadget
/// instance. Called from `enable()` once per re-enable
/// cycle. The gadget-data and gadget-archive paths come
/// from the bridge (the bridge already has them); the
/// XDG-style paths come from Tauri's path resolver, which
/// produces platform-correct values (`Application Support`
/// on macOS, `%APPDATA%` on Windows, `$XDG_CONFIG_HOME`
/// with fallback on Linux).
fn build_path_context(
    app: &tauri::AppHandle,
    gadget_data: &PathBuf,
    gadget_archive: &PathBuf,
) -> anyhow::Result<PathContext> {
    use tauri::Manager;

    let path_resolver = app.path();
    let home = path_resolver.home_dir().context("resolve home directory")?;
    let xdg_config = path_resolver
        .config_dir()
        .context("resolve config directory")?;
    let xdg_data = path_resolver.data_dir().context("resolve data directory")?;

    Ok(PathContext {
        gadget_data: gadget_data.clone(),
        gadget_archive: gadget_archive.clone(),
        home,
        xdg_config,
        xdg_data,
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
    type Caps = WasmGadgetCaps;

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

    fn provision(&self, ctx: &ProvisioningContext) -> anyhow::Result<WasmGadgetCaps> {
        use anyhow::Context;

        let app = &ctx.app;

        let path_context = build_path_context(app, &self.gadget_data, &self.gadget_archive)
            .context("resolve path context")?;

        // Compile command rules against the resolved PathContext.
        let mut compiled_rules = Vec::with_capacity(self.command_rules_raw.len());
        for (index, raw) in self.command_rules_raw.iter().enumerate() {
            match super::argv_matcher::compile_rule(raw, index, &path_context) {
                Ok(rule) => compiled_rules.push(rule),
                Err(e) => {
                    self.log(
                        LogLevel::Error,
                        format!(
                            "compiling command rule {index} for `{}`: {e:#}",
                            self.gadget_id
                        ),
                    );
                }
            }
        }

        // Compile fs allowlist against the resolved PathContext.
        let fs_allowlist = match self.fs_patterns_raw.as_ref() {
            Some(patterns) => {
                match super::runtime::host::fs::compile_fs_patterns(patterns, &path_context) {
                    Ok(allow) => Some(Arc::new(allow)),
                    Err(e) => {
                        self.log(
                            LogLevel::Error,
                            format!("compiling fs allowlist for `{}`: {e:#}", self.gadget_id),
                        );
                        None
                    }
                }
            }
            None => None,
        };

        // Build opener closures from the AppHandle.
        let app_handle = app.clone();
        let clipboard_writer = Box::new(move |text: &str| {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            app_handle
                .clipboard()
                .write_text(text)
                .map_err(|e| format!("write to clipboard: {e}"))
        });

        let opener_handle = app.clone();
        let opener_fn: UrlOpenerFn = Box::new(move |url: &str| {
            use tauri_plugin_opener::OpenerExt;
            opener_handle
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|e| e.to_string())
        });

        let open_path_handle = app.clone();
        let open_path_fn: UrlOpenerFn = Box::new(move |path: &str| {
            use tauri_plugin_opener::OpenerExt;
            open_path_handle
                .opener()
                .open_path(path, None::<&str>)
                .map_err(|e| e.to_string())
        });

        let reveal_path_handle = app.clone();
        let reveal_path_fn: UrlOpenerFn = Box::new(move |path: &str| {
            use tauri_plugin_opener::OpenerExt;
            reveal_path_handle
                .opener()
                .reveal_item_in_dir(path)
                .map_err(|e| e.to_string())
        });

        // Materialize SQL storage from the bridge's cached config.
        let mut sql = SqlState {
            config: self.sql_config.clone(),
            storage: None,
            handle_reps: Vec::new(),
        };
        if let SqlConfig::Configured {
            ref db_path,
            ref migrations,
        } = sql.config
        {
            let migration_strs: Vec<&str> = migrations.iter().map(String::as_str).collect();
            let storage = crate::storage::SqlStorage::open(db_path.clone(), &migration_strs)
                .context("open SQL storage")?;
            sql.storage = Some(Arc::new(storage));
        }

        let settings = GadgetSettings::new(Arc::clone(&ctx.store), self.id());
        let frecency = GadgetFrecency::new(Arc::clone(&ctx.frecency), self.id());

        let website_metadata_enabled =
            self.website_metadata_enabled && self.metadata_service.is_some();

        Ok(WasmGadgetCaps {
            settings: Some(settings),
            frecency: Some(frecency),
            gadget_source: Some(Arc::clone(&self.gadget_source)),
            path_context,
            sql,
            clipboard: ClipboardState {
                writer: Some(clipboard_writer),
            },
            opener: OpenerState {
                schemes: self.opener_schemes.clone(),
                open_path: self.opener_open_path,
                reveal_path: self.opener_reveal_path,
                open_url_writer: Some(opener_fn),
                open_path_writer: Some(open_path_fn),
                reveal_path_writer: Some(reveal_path_fn),
            },
            http: HttpState {
                origins: self.http_origins.clone(),
                client: Some(Arc::new(crate::network::Http::new())),
                insecure_client: Arc::new(std::sync::OnceLock::new()),
            },
            fs: FsState {
                allowlist: fs_allowlist,
            },
            command: CommandState {
                rules: compiled_rules,
            },
            website_metadata: WebsiteMetadataState {
                enabled: website_metadata_enabled,
                service: if website_metadata_enabled {
                    self.metadata_service.clone()
                } else {
                    None
                },
            },
        })
    }

    fn enable(&self, caps: WasmGadgetCaps) {
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

        instance.set_caps(caps);

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
        _app: &tauri::AppHandle,
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
            None,
        )
    }

    /// Build a `WasmGadgetCaps` from a bridge's SQL config
    /// for tests that verify SQL storage across enable cycles.
    fn build_test_caps(bridge: &WasmGadgetBridge) -> WasmGadgetCaps {
        use crate::wasm::runtime::host::sql::SqlState;

        let mut sql = SqlState {
            config: bridge.sql_config.clone(),
            storage: None,
            handle_reps: Vec::new(),
        };
        if let SqlConfig::Configured {
            ref db_path,
            ref migrations,
        } = sql.config
        {
            let migration_strs: Vec<&str> = migrations.iter().map(String::as_str).collect();
            let storage =
                crate::storage::SqlStorage::open(db_path.clone(), &migration_strs).expect("open");
            sql.storage = Some(Arc::new(storage));
        }

        WasmGadgetCaps {
            sql,
            ..WasmGadgetCaps::default_for_test()
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
            None,
        );
        // Construction is lazy — the error surfaces on first
        // acquire/instantiate, not at bridge construction time.
        let bridge = result.expect("lazy construction succeeds");
        assert!(bridge.ensure_instance().is_err());
    }

    #[test]
    fn new_surfaces_missing_migration_file() {
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
        let app_data = tempfile::tempdir().expect("tempdir");

        let result = WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data.path(),
            None,
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
        let bridge = WasmGadgetBridge::new(
            manifest,
            test_runtime(),
            LogContext::test_context(),
            source,
            app_data.path(),
            None,
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
            caps.sql.storage.is_some(),
            "first caps should have SQL storage"
        );
        first.set_caps(caps);
        first.clear_caps();
        drop(first);
        let _ = bridge.take_instance();

        let second = bridge.ensure_instance().expect("second ensure");
        let caps = build_test_caps(&bridge);
        assert!(
            caps.sql.storage.is_some(),
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
            None,
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
            None,
        )
        .expect("bridge construction");

        assert_eq!(bridge.parsed_tasks.len(), 2);
        assert_eq!(bridge.parsed_tasks[0].id, "hourly");
        assert_eq!(bridge.parsed_tasks[1].id, "every-five");
    }
}
