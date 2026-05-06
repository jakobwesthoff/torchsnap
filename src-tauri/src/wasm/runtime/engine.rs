// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WasmRuntime — stateless compile/instantiate service
//
// Wraps the wasmtime `Engine` (one per process) and nothing
// else. Provides three operations:
//
// - `compile(wasm_bytes)` → `Component`
// - `deserialize_component(path)` → `Component`
// - `instantiate(gadget_id, component, &LogContext)`
//   → `WasmGadgetInstance`
//
// Logging plumbing is deliberately not stored on the runtime.
// Callers thread a `LogContext` into `instantiate` so
// `GadgetState` can route guest log items and the
// per-instance `Logger` can be built. Span creation around
// `acquire` and `instantiate` is the orchestrator's
// responsibility — see `CachedComponent`.
// =========================================================

use std::sync::Arc;

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store, Strategy};
use wasmtime_wasi::WasiCtxBuilder;

use crate::wasm::bindings;
use crate::wasm::logging::LogSource;
use crate::wasm::logging::channel::LogContext;

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
}

impl WasmRuntime {
    /// Create a new runtime with default configuration.
    ///
    /// Returns `Arc<Self>` so every `CachedComponent` can
    /// hold a cheap clone for on-demand compilation and
    /// instantiation.
    pub fn new() -> anyhow::Result<Arc<Self>> {
        let mut config = Config::new();
        config.wasm_component_model(true);
        // Winch is a single-pass compiler that produces ~60% less
        // compiled code than Cranelift, cutting idle RSS from ~242 MB
        // to ~110 MB with 7 gadgets loaded. See ADR 0043.
        config.strategy(Strategy::Winch);

        let engine = Engine::new(&config)
            .map_err(|e| anyhow::anyhow!("create wasmtime engine: {e}"))?;
        Ok(Arc::new(Self { engine }))
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
    ///
    /// No span is started here — orchestrators create their
    /// own (`CachedComponent::instantiate` wraps this call in
    /// an `instantiate` child of the `init` span). Tests that
    /// hit this directly need no logging context other than
    /// what `LogContext::test_context()` provides.
    pub fn instantiate(
        &self,
        gadget_id: &str,
        component: &Component,
        log_ctx: &LogContext,
    ) -> anyhow::Result<WasmGadgetInstance> {
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
            log_ctx.sender.clone(),
            Arc::clone(&log_ctx.span_registry),
        );

        let mut store = Store::new(&self.engine, state);

        let gadget = bindings::Gadget::instantiate(&mut store, component, &linker)
            .map_err(|e| anyhow::anyhow!("instantiate WASM gadget: {e}"))?;

        let logger = log_ctx.logger(LogSource::Gadget(gadget_id.to_string()));
        Ok(WasmGadgetInstance::from_parts(store, gadget, logger))
    }
}
