# wasmtime AOT Precompilation Cache

Implement ahead-of-time compilation cache for WASM components using `Engine::precompile_component`. Cache compiled `.cwasm` files alongside (or near) the source archives to reduce startup time.

**Strategy doc:** §9.2 (startup time risk, `Engine::precompile_component`, `.cwasm` caching)

**Status:** needs discussion

**Discussion needed:**
- Cache invalidation strategy: compare source archive mtime? Hash of WASM bytes? A manifest `version` field? The strategy doc notes this as an open question (§6.3 mentions it for frontend assets; same principle applies here).
- Storage location: next to the `.torchsnap` file in `$APPDATA/torchsnap/gadgets/`? A separate `$APPDATA/torchsnap/gadget-cache/<id>/` directory? The latter is cleaner for cleanup.
- Whether precompilation applies only to production `ArchiveSource` loads or also `DirectorySource` dev loads.

**Notes:** Precompilation is a production optimization — don't let it block earlier milestones. Schedule after the full pipeline (at least hello-world + one real gadget) is validated. The `.cwasm` format is wasmtime-version-specific; cache must be invalidated on wasmtime version changes (Cargo.lock change is a reasonable proxy).
