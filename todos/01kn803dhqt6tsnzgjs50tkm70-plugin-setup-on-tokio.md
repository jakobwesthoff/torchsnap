# Refactor: Run plugin setup() on Tokio instead of Rayon

## Problem

Plugin `setup()` currently runs on a Rayon thread pool spawned from a plain
`thread::spawn`. This means there is no Tokio runtime in scope — any code that
needs async (e.g. `Http` using reqwest, or future async plugin APIs) must
explicitly obtain the runtime handle via `tauri::async_runtime::handle()`.

This caused a panic in the bangs plugin when `Http::send()` used
`Handle::current()`, which requires a thread-local Tokio guard that doesn't
exist on Rayon threads.

## Proposed Solution

Replace the Rayon pool in `PluginHost::initialize_and_start()` with Tokio's
`spawn_blocking` for parallel plugin setup. Each plugin's `setup()` would run
inside `spawn_blocking`, which:

1. Gives plugins a Tokio runtime context (`Handle::current()` works)
2. Maintains parallelism (spawn_blocking threads run concurrently)
3. Allows plugins to call `.block_on()` for async operations naturally
4. Removes the need for the explicit `tauri::async_runtime::handle()` workaround
   in `Http`

## Considerations

- Rayon's thread pool is sized at 70% of cores. Tokio's blocking pool grows
  on demand (default max 512 threads). For 7 plugins this is fine, but worth
  noting the different concurrency model.
- `spawn_blocking` returns `JoinHandle<T>` — the host can `await` all handles
  to know when setup is complete, which is cleaner than the current fire-and-forget
  `thread::spawn`.
- The `Plugin::setup()` signature stays synchronous (`fn setup(&self, ...)`) —
  `spawn_blocking` is the right tool for running sync code on the Tokio runtime.
