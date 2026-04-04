# Hello-World Integration

Wire the complete pipeline end-to-end: load hello-world via `DirectorySource`, instantiate via `WasmRuntime`, wrap in `WasmPluginBridge`, register with `PluginHost`, and verify the plugin appears in search results and `execute` works.

**Strategy doc:** §8.2 (Phase 0 rationale), §8.5 milestone 5 (integration), §8.3 (step-by-step process)

**Status:** not started

**Depends on:** `01knacefg46mz1wgywpkn8h97g` (plugin-discovery)

**Notes:** This is the end-to-end validation milestone for the entire WASM infrastructure before any real plugin is touched. Success criteria: hello-world entry appears in launcher search, clicking execute logs output visible via tracing, no panics or instantiation errors. Also validates that the logging host import works (guest `println!` and `logging::log()` both surface in host logs tagged with the plugin ID).
