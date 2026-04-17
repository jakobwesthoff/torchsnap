# Plugin Discovery

**Status:** superseded by ADR 0035 (plugin distribution via
bundled and user-installable archives) and its implementation
across commits covering the dual-path loader, the
`plugins/bundled.toml` whitelist, and the install/uninstall flow.

The final discovery scheme lives at
`docs/adr/0035-plugin-distribution-via-bundled-and-user-installable-archives.md`.
Three precedence-ordered search roots (System, Dev, User), per-root
archive-over-directory rule, cross-root first-wins with warning,
and `PluginSourceKind` plumbing through `PluginHost`.

The disabled-plugin optimization mentioned below remains a valid
follow-up: today the loader instantiates every discovered plugin
unconditionally. A future pass can skip instantiation for plugins
whose `enabled.<id>` is `false`, parsing only the manifest.

---

Historical context (for reference):

Scan plugin directories at startup, load manifests via `DirectorySource` (and eventually `ArchiveSource`), instantiate enabled plugins, and register them with `PluginHost`.

**Strategy doc:** §6.1 (directory layout, scan-at-startup flow), §8.4 (no cargo workspace; dev vs production paths)

**Depends on:** `01knacefg46mz1wgywpkn8h97f` (wasm-plugin-bridge)

**Discussion needed:** Where does the plugin directory path come from? Options:
- Hardcoded dev path (e.g., `../plugins/`) during the initial hello-world phase
- A config key in `tauri.conf.json` or a Tauri-managed app config
- CLI flag / environment variable for development overrides
- Production path `$APPDATA/torchsnap/plugins/` only

The disabled-plugin optimization from §4.2 applies here: only parse the manifest for disabled plugins; skip WASM instantiation entirely.
