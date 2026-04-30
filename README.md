# Torchsnap

**Light. Find. Launch.**

Strike a key, light the way. A cross-platform launcher to find, launch,
and automate anything across every system.

Torchsnap is a Tauri 2 desktop application (Rust host, React + Vite
frontend) with a sandboxed WebAssembly plugin system. Plugins are
WASM components targeting `wasm32-wasip2`, distributed as
single-file `.torchsnap` archives. The host (`src-tauri/`) loads them
at runtime via `wasmtime`, mediates every host capability through a
manifest-declared permission model, and streams search results into
the launcher UI.

The primary supported platform today is macOS.

## Installing plugins

Third-party plugins ship as `.torchsnap` files — single-file
archives you can hand-share. To install one:

1. Open **Settings → Plugins**.
2. Click **Choose a file…** and pick the `.torchsnap`, or drop the
   file onto the drop zone.
3. When prompted, click **Restart now**. The plugin is active after
   restart.

Installed user plugins live under
`<app_data_dir>/plugins/<id>.torchsnap` and their host-managed state
(SQLite databases, caches) under
`<app_data_dir>/plugin-home/<id>/`. Uninstalling a plugin from the
Plugins panel removes both.

Plugins bundled with the app (like the calculator) cannot be
uninstalled — they are upgraded along with Torchsnap itself.

## Developing plugins

See [`docs/api/plugin-development.md`](docs/api/plugin-development.md)
for the plugin author guide: the WIT interface, manifest format,
discovery rules, and packaging instructions.

For the host-side architecture (component model, lifecycle, search
dispatch, host capabilities, frontend integration), start with
[`docs/Plugin-Architecture/01-overview.md`](docs/Plugin-Architecture/01-overview.md).
Design rationale for individual subsystems lives in
[`docs/adr/`](docs/adr/).

## Building from source

Torchsnap uses [`just`](https://github.com/casey/just) as its task
runner and [`bun`](https://bun.sh) for the frontend. Plugins are
built with plain `cargo build --release` against the
`wasm32-wasip2` target — `cargo-component` is **not** used. WIT
inspection and formatting use `wasm-tools`.

Common recipes (run `just --list` for the full set):

- `just build` — full release build. Stages whitelisted plugins
  from `plugins/bundled.toml` into `target/bundled-plugins/`,
  builds every in-tree plugin, then runs `tauri build`.
- `just build profile=debug` — same flow with `tauri build --debug`.
- `just build-plugin <name>` — rebuild a single plugin under
  `plugins/<name>/` and repackage it as `<name>.torchsnap`.
- `just check-plugins` / `just check-wit` — fast workspace and WIT
  validation.

The repo-root `target/` directory is owned by the bundled-plugin
staging flow and is gitignored. Cargo's own build outputs go to
`src-tauri/target/` (host) and `plugins/target/` (plugin
workspace).

## License

Mozilla Public License Version 2.0 (MPL-2.0). Every source file
carries the standard MPL header — see `CLAUDE.md` for the per-
language header formats.
