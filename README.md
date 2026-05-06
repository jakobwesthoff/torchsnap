# Torchsnap

**Light. Find. Launch.**

Strike a key, light the way. A cross-platform launcher to find, launch,
and automate anything across every system.

Torchsnap is a Tauri 2 desktop application (Rust host, React + Vite
frontend) with a sandboxed WebAssembly gadget system. Gadgets are
WASM components targeting `wasm32-wasip2`, distributed as
single-file `.torchsnap` archives. The host (`src-tauri/`) loads them
at runtime via `wasmtime`, mediates every host capability through a
manifest-declared permission model, and streams search results into
the launcher UI.

The primary supported platform today is macOS.

## Installing gadgets

Third-party gadgets ship as `.torchsnap` files — single-file
archives you can hand-share. To install one:

1. Open **Settings → Gadgets**.
2. Click **Choose a file…** and pick the `.torchsnap`, or drop the
   file onto the drop zone.
3. When prompted, click **Restart now**. The gadget is active after
   restart.

Installed user gadgets live under
`<app_data_dir>/gadgets/<id>.torchsnap` and their host-managed state
(SQLite databases, caches) under
`<app_data_dir>/gadget-home/<id>/`. Uninstalling a gadget from the
Gadgets panel removes both.

Gadgets bundled with the app (like the calculator) cannot be
uninstalled — they are upgraded along with Torchsnap itself.

## Developing gadgets

See [`docs/api/gadget-development.md`](docs/api/gadget-development.md)
for the gadget author guide: the WIT interface, manifest format,
discovery rules, and packaging instructions.

For the host-side architecture (component model, lifecycle, search
dispatch, host capabilities, frontend integration), start with
[`docs/Gadget-Architecture/01-overview.md`](docs/Gadget-Architecture/01-overview.md).
Design rationale for individual subsystems lives in
[`docs/adr/`](docs/adr/).

## Building from source

Torchsnap uses [`just`](https://github.com/casey/just) as its task
runner and [`bun`](https://bun.sh) for the frontend. Gadgets are
built with plain `cargo build --release` against the
`wasm32-wasip2` target — `cargo-component` is **not** used. WIT
inspection and formatting use `wasm-tools`.

Common recipes (run `just --list` for the full set):

- `just build` — full release build. Stages whitelisted gadgets
  from `gadgets/bundled.toml` into `target/bundled-gadgets/`,
  builds every in-tree gadget, then runs `tauri build`.
- `just build profile=debug` — same flow with `tauri build --debug`.
- `just build-gadget <name>` — rebuild a single gadget under
  `gadgets/<name>/` and repackage it as `<name>.torchsnap`.
- `just check-gadgets` / `just check-wit` — fast workspace and WIT
  validation.

The repo-root `target/` directory is owned by the bundled-gadget
staging flow and is gitignored. Cargo's own build outputs go to
`src-tauri/target/` (host) and `gadgets/target/` (gadget
workspace).

## License

Mozilla Public License Version 2.0 (MPL-2.0). Every source file
carries the standard MPL header — see `CLAUDE.md` for the per-
language header formats.
