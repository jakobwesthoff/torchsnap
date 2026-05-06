// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WasmRuntime — stateless compile/instantiate service
//
// Holds the wasmtime `Engine` (one per process) and shared
// logging infrastructure. Provides two operations:
//
// - `compile(wasm_bytes)` → `Component`
// - `instantiate(gadget_id, &component)` → `WasmGadgetInstance`
//
// The runtime does not store components — callers own the
// `Component` lifecycle. `CachedComponent` wraps this
// runtime with a disk-backed cache and acquire/release
// semantics for in-memory residency.
// =========================================================

use std::sync::Arc;

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, Strategy};
use wasmtime_wasi::WasiCtxBuilder;

use crate::wasm::bindings;
use crate::wasm::logging::LogSource;
use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::spans::{Logger, SpanRegistry};

use super::instance::WasmGadgetInstance;
use super::state::GadgetState;

/// Shared WASM runtime providing compilation and
/// instantiation as a stateless service.
///
/// One instance per application. Does not store compiled
/// components — the caller (`CachedComponent` in production,
/// tests directly) owns the `Component` lifetime.
pub struct WasmRuntime {
    engine: Engine,
    log_sender: LogSender,
    span_registry: Arc<SpanRegistry>,
}

impl WasmRuntime {
    /// Create a new runtime with default configuration.
    ///
    /// Returns `Arc<Self>` so every `CachedComponent` can
    /// hold a cheap clone for on-demand compilation and
    /// instantiation.
    pub fn new(
        log_sender: LogSender,
        span_registry: Arc<SpanRegistry>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        // Winch is a single-pass compiler that produces ~60% less
        // compiled code than Cranelift, cutting idle RSS from ~242 MB
        // to ~110 MB with 7 gadgets loaded. See ADR 0043.
        config.strategy(Strategy::Winch);

        let engine = Engine::new(&config)
            .map_err(|e| anyhow::anyhow!("create wasmtime engine: {e}"))?;
        Ok(Arc::new(Self {
            engine,
            log_sender,
            span_registry,
        }))
    }

    /// Deserialize a compiled component from a cache file.
    ///
    /// The file must contain bytes previously produced by
    /// `Component::serialize()` for the same `Engine`
    /// configuration. This is the cache-hit path used by
    /// `CachedComponent`.
    ///
    /// # Safety
    ///
    /// `Component::deserialize_file` is `unsafe` because it
    /// trusts the serialized byte stream. The caller must
    /// ensure the file was written by us from
    /// `Component::serialize()` for an equivalent engine.
    pub fn deserialize_component(&self, path: &std::path::Path) -> anyhow::Result<Component> {
        // SAFETY: caller guarantees the file was written by
        // `Component::serialize()` for this engine config.
        unsafe { Component::deserialize_file(&self.engine, path) }
            .map_err(|e| anyhow::anyhow!("deserialize compiled component: {e}"))
    }

    /// Serialize a compiled component to bytes suitable for
    /// `deserialize_file`. Wraps the wasmtime API to keep
    /// all wasmtime interactions behind `WasmRuntime`.
    pub fn serialize_component(component: &Component) -> anyhow::Result<Vec<u8>> {
        component.serialize()
            .map_err(|e| anyhow::anyhow!("serialize compiled component: {e}"))
    }

    /// Compute a hex string representing the engine's
    /// precompile compatibility configuration. Two engines
    /// produce compatible serialized components iff their
    /// compatibility hashes match.
    pub fn precompile_compatibility_hex(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        self.engine.precompile_compatibility_hash().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Create a Logger for a specific gadget, used for
    /// host-side span creation around guest calls.
    fn logger_for(&self, gadget_id: &str) -> Logger {
        Logger::new(
            self.log_sender.clone(),
            Arc::clone(&self.span_registry),
            LogSource::Gadget(gadget_id.to_string()),
        )
    }

    /// Compile WASM bytes into a wasmtime `Component`.
    ///
    /// The returned `Component` is heap-allocated. Callers
    /// that want file-backed (mmap'd) components should use
    /// `CachedComponent`, which calls this method on cache
    /// miss, serializes the result to disk, and deserializes
    /// back as a file-backed component.
    pub fn compile(&self, wasm_bytes: &[u8]) -> anyhow::Result<Component> {
        Component::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("compile WASM component: {e}"))
    }

    /// Instantiate a compiled component into a ready-to-call
    /// gadget instance.
    ///
    /// Builds a fresh Linker, WasiCtx, GadgetState, and Store
    /// against the provided `Component`. The linker is rebuilt
    /// on every call because it references `GadgetState`, but
    /// construction is just map inserts (no WASM compilation)
    /// and this method is not in a hot path.
    pub fn instantiate(
        &self,
        gadget_id: &str,
        component: &Component,
    ) -> anyhow::Result<WasmGadgetInstance> {
        let logger = self.logger_for(gadget_id);
        let _instantiate_span = logger
            .span("instantiate")
            .meta("gadget_id", gadget_id)
            .start();

        let mut linker = Linker::<GadgetState>::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| anyhow::anyhow!("link WASI imports: {e}"))?;
        bindings::Gadget::add_to_linker::<GadgetState, HasSelf<GadgetState>>(
            &mut linker,
            |state| state,
        )
        .map_err(|e| anyhow::anyhow!("link gadget imports: {e}"))?;

        // Minimal sandbox: no filesystem, no network, no env vars.
        // Stdout/stderr inherited so gadget println! reaches the host terminal.
        // TODO(redirect-gadget-stdout): see todos/gadget-host/wasm/01kqmdf6rgavhar6m4mm8vhxeb-redirect-gadget-stdout-to-logger.md
        let wasi = WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();

        let state = GadgetState::new(
            gadget_id.to_string(),
            wasi,
            self.log_sender.clone(),
            Arc::clone(&self.span_registry),
        );

        let mut store = Store::new(&self.engine, state);

        let gadget = bindings::Gadget::instantiate(&mut store, component, &linker)
            .map_err(|e| anyhow::anyhow!("instantiate WASM gadget: {e}"))?;

        Ok(WasmGadgetInstance::from_parts(store, gadget, logger))
    }
}
