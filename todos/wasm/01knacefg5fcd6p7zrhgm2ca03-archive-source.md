# ArchiveSource

Implement `ArchiveSource`: reads `manifest.toml`, WASM binary, and frontend assets from a `.torchsnap` zip archive. Provides the same `PluginSource` interface as `DirectorySource`.

**Strategy doc:** §4.1 (archive format and file layout), §4.3 (why zip), §4.4 (source abstraction, `PluginSource` trait)

**Status:** not started

**Depends on:** `zip` crate addition (`cargo add zip` in `src-tauri/`)

**Notes:** WASM binary is read by the manifest-declared path (`wasm` field in `[plugin]`), not hardcoded. Frontend assets need to be extractable to a cache directory (`$APPDATA/torchsnap/plugin-cache/<id>/`) so the WebView can load them via `asset://` — but frontend extraction can be deferred until `frontend-dynamic-loading`. For this task, focus on manifest + WASM binary access. The archive can be read in-memory without full extraction for those two files.
