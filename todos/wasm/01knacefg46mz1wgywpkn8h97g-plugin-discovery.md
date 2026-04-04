# Plugin Discovery

Scan plugin directories at startup, load manifests via `DirectorySource` (and eventually `ArchiveSource`), instantiate enabled plugins, and register them with `PluginHost`.

**Strategy doc:** §6.1 (directory layout, scan-at-startup flow), §8.4 (no cargo workspace; dev vs production paths)

**Status:** needs discussion

**Depends on:** `01knacefg46mz1wgywpkn8h97f` (wasm-plugin-bridge)

**Discussion needed:** Where does the plugin directory path come from? Options:
- Hardcoded dev path (e.g., `../plugins/`) during the initial hello-world phase
- A config key in `tauri.conf.json` or a Tauri-managed app config
- CLI flag / environment variable for development overrides
- Production path `$APPDATA/torchsnap/plugins/` only

The disabled-plugin optimization from §4.2 applies here: only parse the manifest for disabled plugins; skip WASM instantiation entirely.
