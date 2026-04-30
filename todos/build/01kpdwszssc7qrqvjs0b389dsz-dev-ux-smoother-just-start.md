# Smoother Dev UX / DX — `just start` and friends

The day-to-day developer loop still carries too much manual
ceremony. Starting the app for development frequently requires
remembering which artifacts need rebuilding in which order before
`just start` (or the Tauri dev command) will actually launch
something that works end-to-end. The plugin-related additions from
ADR 0035 make this worse: bundled plugins, dev plugins, and user
plugins all have different build triggers that should "just work"
from a single high-level command.

**Status:** needs discussion

## Pain points (from the current workflow)

- `just start` does not (re)build plugins automatically. If a
  plugin's source changed but the `.wasm` wasn't rebuilt, the
  running app picks up stale behaviour silently.
- Plugin frontends require their own `bun run build` before the
  wrapping app will load them. A developer editing a plugin's
  React view has to remember two build steps.
- `just build-plugins` is all-or-nothing — iterating on one
  plugin still rebuilds every other plugin's crate, even ones
  that didn't change.
- No watch mode for plugins. Contrast with the Tauri dev server
  which gives instant frontend HMR for the host app — plugin
  authors get nothing comparable.
- Test fixtures (`src-tauri/tests/fixtures/*/`) have their own
  manual rebuild recipe that's easy to forget when the shared
  WIT world changes.
- No `just doctor` check for plugin health (missing `.wasm`,
  stale `.torchsnap`, etc.). The old `just/doctor.just` exists
  but does not cover the plugin pipeline.

## Proposed directions (to discuss)

- `just start` becomes a single entry point that detects what's
  stale, builds only what's needed, and launches the app. The
  "stale" detection can rely on cargo for Rust crates and file
  mtimes for assets; no fancy build system required.
- Per-plugin watch recipe (`just watch-plugin <name>`) that
  rebuilds the plugin's WASM and frontend on change and
  triggers the app to reload (coordinating with the
  hot-lifecycle todo).
- Global `just watch` that covers the host app *and* every
  plugin checked into `plugins/`. Two separate watch loops
  running in parallel; one hotkey or signal to terminate both.
- First-run bootstrap: a `just doctor` recipe that inspects
  toolchain presence (rust, bun, wasm-tools, wasm32-wasip2
  target), plugin build state, and WIT formatting — pointing at
  the fix command for each problem it finds.
- Hook plugin build into cargo's workspace invalidation if
  practical (so `cargo run` rebuilds plugin `.wasm` when their
  source changed, without a separate Just invocation).

## Dependencies / related todos

- The hot-lifecycle work
  (`todos/wasm/…-plugin-hot-lifecycle.md`) unlocks much of the
  value here: without hot reload, a smoother build-and-launch
  still requires manual app restarts for plugin changes.
- The Justfile-extraction todo
  (`todos/…-extract-complex-justfile-logic.md`) argues that
  any non-trivial build logic should move out of the Justfile
  anyway; any implementation of this todo should respect that
  direction.
- The test-fixture build recipe
  (`just build-test-fixtures`) should be folded into the same
  "build what's stale" machinery.

## Open questions

- How much incrementality is worth chasing? For small repos a
  stupid-simple "always rebuild everything" is fine; beyond a
  handful of plugins the cost hurts. Pick a threshold.
- Should the host app's dev server proxy plugin frontend dev
  servers too, so plugin authors get HMR for their own React
  code? That's a larger architectural decision than it looks.
- Is it worth formalizing a `tools/dev` workspace crate that
  owns the watcher + builder logic, and a thin Just recipe
  that calls into it?
