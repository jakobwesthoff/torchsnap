// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WasmGadgetInstance — one per loaded gadget
//
// Wraps the wasmtime `Store<GadgetState>` plus the typed
// `bindings::Gadget` and a host-side `Logger`. Per-call
// guest dispatch (enable / disable / search / execute /
// handle_message / run_task / on_setting_changed) lives
// here.
//
// Capability-specific setters/clearers
// (`set_clipboard_writer`, `set_opener_schemes`, etc.)
// land via `impl WasmGadgetInstance` extension blocks in
// the `host/<capability>.rs` files. The foundational
// setters defined here cover the cross-capability state
// (`gadget_source`, `path_context`) plus the
// `with_state_mut` chokepoint every other setter routes
// through.
// =========================================================

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use wasmtime::Store;

use crate::wasm::bindings;
use crate::wasm::logging::spans::Logger;
use crate::wasm::permission_vars::PathContext;
use crate::wasm::source::GadgetSource;

use super::state::GadgetState;

/// A loaded WASM gadget instance.
///
/// Wraps the wasmtime Store and typed Gadget bindings. All
/// guest calls go through the Mutex-protected Store to
/// satisfy `Send + Sync` requirements.
///
/// `search_generation` powers a per-instance "latest-wins"
/// elision pattern around the store mutex. WASM components
/// are single-threaded by spec, so search calls serialize on
/// `store.lock()`. When a guest's host import blocks for
/// seconds (e.g. `website-metadata::lookup` in `Blocking`
/// mode), every later keystroke's search call queues on
/// the mutex behind it. Without elision, FIFO drains every
/// queued call after the slow one releases — wasting compute
/// on results the frontend would discard by generation
/// anyway, and delaying the user's *current* query's call
/// behind a chain of stale ones. Each `search()` entry
/// monotonically increments this counter and re-reads it
/// after acquiring the mutex; if a newer call registered
/// during the wait, the older one short-circuits with an
/// empty result instead of running.
pub struct WasmGadgetInstance {
    pub(crate) store: Mutex<Store<GadgetState>>,
    pub(crate) gadget: bindings::Gadget,
    pub(crate) logger: Logger,
    pub(crate) search_generation: AtomicU64,
}

impl WasmGadgetInstance {
    /// Build a `WasmGadgetInstance` from the parts produced
    /// by `WasmRuntime::instantiate`. Crate-private — the
    /// only legitimate caller is the engine.
    pub(crate) fn from_parts(
        store: Store<GadgetState>,
        gadget: bindings::Gadget,
        logger: Logger,
    ) -> Self {
        Self {
            store: Mutex::new(store),
            gadget,
            logger,
            search_generation: AtomicU64::new(0),
        }
    }

    /// Apply a closure to a mutable reference to the gadget's
    /// `GadgetState`, holding the store lock for its duration.
    /// The single chokepoint every capability setter routes
    /// through, so the lock-acquire / `data_mut()` pattern
    /// lives in one place rather than 30 setters.
    pub(crate) fn with_state_mut<R>(&self, f: impl FnOnce(&mut GadgetState) -> R) -> R {
        let mut store = self.store.lock().expect("store not poisoned");
        f(store.data_mut())
    }
}

// =========================================================
// Foundational setters — cross-capability state
//
// `path_context` is consumed by both `paths::resolve` and
// `command::run`; `gadget_source` is consumed by
// `assets::*`. Both are bridge-stashed at `enable()`.
// =========================================================

impl WasmGadgetInstance {
    /// Stash the resolved `${...}` substitution context.
    /// Called by the bridge at `enable()` after computing the
    /// per-gadget paths.
    pub fn set_path_context(&self, ctx: PathContext) {
        self.with_state_mut(|state| state.path_context = Some(ctx));
    }

    /// Drop the substitution context on `disable()`.
    pub fn clear_path_context(&self) {
        self.with_state_mut(|state| state.path_context = None);
    }

    /// Stash the gadget's own `GadgetSource` handle. Called
    /// by the bridge on `enable()`. The `assets::*` host
    /// imports use this Arc to read the gadget's bundled
    /// files on demand.
    pub fn set_gadget_source(&self, source: Arc<dyn GadgetSource + Send + Sync>) {
        self.with_state_mut(|state| state.gadget_source = Some(source));
    }

    /// Drop the gadget source on `disable()`. Eager release
    /// so the underlying `ArchiveSource` file handle (or
    /// the `DirectorySource` path) doesn't linger across
    /// enable cycles.
    pub fn clear_gadget_source(&self) {
        self.with_state_mut(|state| state.gadget_source = None);
    }
}

// =========================================================
// Guest call dispatch
//
// Each method here invokes one WIT export on the guest.
// The outer `Result` is for wasmtime trap / serialization
// errors; for exports that return their own `Result` (e.g.
// `handle-message`, `run-task`), the inner result is the
// gadget's success/error arm.
//
// Every call holds the store mutex for its full duration —
// that is the ordering invariant the host import
// implementations rely on (no concurrent access to
// `GadgetState` is possible while a guest call is running).
// =========================================================

impl WasmGadgetInstance {
    /// Call the guest's `enable` export.
    pub fn enable(&self) -> anyhow::Result<()> {
        let _span = self.logger.span("enable").start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.gadget
            .torchsnap_gadget_lifecycle()
            .call_enable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling gadget enable(): {e}"))
    }

    /// Call the guest's `disable` export.
    pub fn disable(&self) -> anyhow::Result<()> {
        let _span = self.logger.span("disable").start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.gadget
            .torchsnap_gadget_lifecycle()
            .call_disable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling gadget disable(): {e}"))
    }

    /// Call the guest's `on-setting-changed` export. Used by
    /// the bridge's `setting_changed` trait override after the
    /// host's `CoalescingDispatcher` has deduplicated rapid
    /// writes to the same key.
    pub fn on_setting_changed(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let _span = self
            .logger
            .span("on_setting_changed")
            .meta("key", key)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.gadget
            .torchsnap_gadget_lifecycle()
            .call_on_setting_changed(&mut *store, key, value)
            .map_err(|e| anyhow::anyhow!("calling gadget on_setting_changed(): {e}"))
    }

    /// Call the guest's `tasks::run-task` export.
    ///
    /// Used by the per-gadget scheduler loop to fire a
    /// scheduled task. The `task_id` matches a `[[tasks]]`
    /// entry from the manifest. The gadget dispatches by
    /// name and runs whatever work the task is supposed to
    /// do.
    ///
    /// The outer `Result` is for wasmtime trap /
    /// serialization errors; the inner
    /// `Result<(), String>` is the gadget's own
    /// success/error arm. Returning `Err(string)` is
    /// logged by the bridge — it does not auto-disable the
    /// gadget.
    #[allow(clippy::type_complexity)]
    pub fn run_task(&self, task_id: &str) -> anyhow::Result<Result<(), String>> {
        let _span = self
            .logger
            .span("run_task")
            .meta("task_id", task_id)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.gadget
            .torchsnap_gadget_tasks()
            .call_run_task(&mut *store, task_id)
            .map_err(|e| anyhow::anyhow!("calling gadget run_task(): {e}"))
    }

    /// Call the guest's `messaging::handle-message` export.
    ///
    /// `payload` is a JSON-encoded string (the gadget parses
    /// it on its side). The returned outer `Result` is for
    /// wasmtime trap / serialization errors; the inner
    /// `Result<String, String>` is the gadget's own
    /// success/error arm. The success arm is the
    /// JSON-encoded response string.
    #[allow(clippy::type_complexity)]
    pub fn handle_message(
        &self,
        method: &str,
        payload: &str,
    ) -> anyhow::Result<Result<String, String>> {
        let _span = self
            .logger
            .span("handle_message")
            .meta("method", method)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.gadget
            .torchsnap_gadget_messaging()
            .call_handle_message(&mut *store, method, payload)
            .map_err(|e| anyhow::anyhow!("calling gadget handle_message(): {e}"))
    }

    /// Call the guest's `entries` export and convert to native types.
    pub fn entries(&self) -> anyhow::Result<Vec<crate::commands::types::CatalogEntry>> {
        let _span = self.logger.span("entries").start();
        let mut store = self.store.lock().expect("store not poisoned");
        let wit_entries = self
            .gadget
            .torchsnap_gadget_search()
            .call_entries(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling gadget entries(): {e}"))?;

        Ok(wit_entries.into_iter().map(Into::into).collect())
    }

    /// Call the guest's `search` export and convert to native types.
    ///
    /// Implements the latest-wins elision described on
    /// [`WasmGadgetInstance`]: each call registers its own
    /// generation, then re-checks after acquiring the store
    /// mutex. When a newer call has overtaken us during the
    /// wait — typical when a previous call is mid-way through
    /// a long blocking host import — we return an empty
    /// `Results` payload instead of executing the stale guest
    /// call. The frontend's per-search generation check
    /// (`useSearch.ts`) would discard our result anyway, so
    /// the work would be pure waste and would block the
    /// user's actually-current query from progressing.
    pub fn search(
        &self,
        query: &str,
        matched_prefix: Option<&str>,
    ) -> anyhow::Result<crate::commands::types::GadgetResponse> {
        let my_gen = self.search_generation.fetch_add(1, Ordering::AcqRel) + 1;

        let _span = self.logger.span("search").meta("query", query).start();
        let mut store = self.store.lock().expect("store not poisoned");

        if self.search_generation.load(Ordering::Acquire) > my_gen {
            return Ok(crate::commands::types::GadgetResponse::Results(vec![]));
        }

        let response = self
            .gadget
            .torchsnap_gadget_search()
            .call_search(&mut *store, query, matched_prefix)
            .map_err(|e| anyhow::anyhow!("calling gadget search(): {e}"))?;

        Ok(response.into())
    }

    /// Call the guest's `execute` export and convert to native types.
    pub fn execute(
        &self,
        entry_id: &str,
        action_id: &crate::commands::types::ActionId,
    ) -> anyhow::Result<crate::commands::types::PostAction> {
        let _span = self
            .logger
            .span("execute")
            .meta("entry_id", entry_id)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");

        let wit_action_id: bindings::exports::torchsnap::gadget::search::ActionId =
            action_id.clone().into();

        let result = self
            .gadget
            .torchsnap_gadget_search()
            .call_execute(&mut *store, entry_id, &wit_action_id)
            .map_err(|e| anyhow::anyhow!("calling gadget execute(): {e}"))?;

        match result {
            Ok(post_action) => Ok(post_action.into()),
            Err(msg) => anyhow::bail!("gadget execute() returned error: {msg}"),
        }
    }
}
