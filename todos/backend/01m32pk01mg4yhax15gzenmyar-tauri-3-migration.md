# Track the Tauri 3 migration

**Kind:** dependency upgrade, tracking
**Status:** watching; Tauri 3 is in alpha

This is the single todo that tracks the move from Tauri 2 to Tauri 3,
from the first alpha until the migration makes sense. Add new findings
here (new pre-releases, changed blockers, trial results) instead of
opening separate todos, and delete it once the migration has landed.

## State as of 2026-09-21

Torchsnap is on Tauri 2.11.6 with matching 2.x plugins (JS and Rust).
Tauri 3 pre-releases so far, per crates.io and npm:

| Package | Pre-release | Published |
|---------|-------------|-----------|
| `tauri` | 3.0.0-alpha.0 | 2026-09-13 |
| `tauri` | 3.0.0-alpha.1 | 2026-09-15 |
| `tauri` | 3.0.0-alpha.2 | 2026-09-21 |
| `tauri-build`, all eight plugins used here | 3.0.0-alpha.1 | |
| `@tauri-apps/api` (npm `next`) | 3.0.0-alpha.1 | |
| `@tauri-apps/cli` (npm `next`) | 3.0.0-alpha.2 | |

The eight plugins: autostart, clipboard-manager, dialog, global-shortcut,
opener, os, process, store. `tauri-plugin-fs` comes in transitively
and was not checked for a 3.x pre-release.

## Why it matters

- **macOS 27 tray fix.** `tauri` 3.0.0-alpha.2 requires `tray-icon ^0.25`
  and `muda ^0.20`. Tauri 2.11.6 requires `tray-icon ^0.24`. tray-icon
  0.25.1 fixes the left-click regression in
  `todos/platform/01m32g17gmgeej8y2q87t3q3pd-macos-27-tray-left-click.md`.
  Until Tauri 3 is usable, that todo's `[patch.crates-io]` workaround is
  the way to get the fix on Tauri 2.

## Blockers

1. **Alpha status.** Breaking changes continue between alphas (alpha.2
   renamed plugin APIs). Wait for a beta or release candidate.
2. **tauri-nspanel.** The launcher panel depends on it. Its latest
   release, 2.1.0, requires `tauri ^2.8.5`, and its repository
   (github.com/ahkohd/tauri-nspanel) had no v3 branch on 2026-09-21. The
   migration needs a Tauri 3 compatible release of it.

## Changes that affect torchsnap

From the GitHub release notes of tauri-v3.0.0-alpha.0 and alpha.2:

- **`macos-private-api` moves to the runtime crate.** The `devtools`,
  `macos-private-api` and `unstable` features must be enabled on
  `tauri-runtime-wry` (or `tauri-runtime-cef`).
  `src-tauri/Cargo.toml` enables `macos-private-api` on `tauri`.
- **Resources are no longer copied at build time.** `tauri-build` stops
  copying the configured resources into the target directory. For
  unbundled runs (`tauri dev`, `cargo run`), plain relative resources are
  read from their source paths. Map notation or `../` paths are mirrored
  next to the executable on first access. Torchsnap uses exactly that:
  `"../target/bundled-gadgets/": "gadgets/"` in `tauri.conf.json`, plus
  `src-tauri/build.rs`, which creates the directory so an empty staging
  directory builds (the change that made fresh checkouts compile). Re-check
  that `build.rs` step, dev-mode gadget discovery
  (`src-tauri/src/wasm/discovery.rs`) and the release bundle's
  `Resources/gadgets/` listing.
- **ACL format.** Command permissions are stored as a `commands` list
  per plugin or app manifest, with implicit `allow-*` / `deny-*`
  permissions. Check `src-tauri/capabilities/default.json`.
- **MSRV 1.95.** Covered by `rust-toolchain.toml` (1.98.1).
- **Plugin author APIs renamed** (`js_init_script` ->
  `initialization_script`, `extend_api` -> `run_invoke_handler`,
  `Invoke::state` removed). Torchsnap defines no Tauri plugins of its own
  (no `plugin::Builder` or `Plugin` impl in `src-tauri/src`), so these do
  not apply directly.
- Android binding and custom-protocol URL changes do not affect a
  macOS-only build.

## When to act

Re-check at each new Tauri 3 pre-release, and start a trial branch once
there is a beta or RC and a Tauri 3 compatible tauri-nspanel. The trial:

1. Bump `tauri`, `tauri-build`, all plugins (Rust) and `@tauri-apps/*`
   (JS) together; Tauri checks that JS and Rust versions agree.
2. Move `macos-private-api` to `tauri-runtime-wry`.
3. Work through compile errors, then `just fullcycle`, `just audit`,
   `just build --release`.
4. Smoke test the release build without touching the real profile:
   - Seed `<scratch>/Library/Application Support/app.torchsnap/settings.json`
     with `{"controlChannel.enabled": true}` and point `HOME` at a short
     symlink to `<scratch>` (the control socket path must stay under
     macOS's 104-byte Unix socket limit).
   - Quit the installed Torchsnap (it owns the global hotkey), launch
     `HOME=<symlink> .../torchsnap.app/Contents/MacOS/torchsnap`.
   - Drive the launcher over the JSON-RPC Control API at
     `<app_data_dir>/control.sock` (`show`, `query {"text": ...}`,
     `status`, `hide`), send real keys with
     `osascript -e 'tell application "System Events" to key code 53'`,
     and check results with `screencapture`.
   - Check: panel shows and takes keyboard input without activating the
     app, Escape clears then hides, focus loss hides, every bundled
     gadget answers a query, a left click on the tray icon behaves as
     intended, and Settings opens from the tray menu.
5. Compare `Resources/gadgets/` in the bundle with the previous release.

Background: the dependency update work of 2026-09-21 (branch
`update-dependencies`) left every direct dependency on its newest stable
release; Tauri 3 was the largest item outside the declared ranges.
