# Minimal Test Gadget Fixture

Test-only WASM gadget used by the Rust test suite in
`src-tauri/src/wasm/runtime.rs` and `src-tauri/src/wasm/bridge.rs`.

## What it is

A standalone `wasm32-wasip2` crate that implements every guest export of
the shared WIT world (`lifecycle`, `search`, `messaging`, `tasks`) with a
no-op body. It exists solely to give the bridge / runtime tests a real,
compilable component they can instantiate and call into without
triggering any meaningful guest logic.

The canonical artifact consumed by tests is the committed
`minimal_gadget.wasm` sitting next to this README. Tests load it either
via `include_bytes!` (for raw bytes passed to `WasmRuntime::compile`) or
by pointing a `DirectorySource` at this directory (for end-to-end
bridge construction tests that need a real `GadgetSource` + manifest).

## When to rebuild

Rebuild this fixture whenever:

- The shared WIT world at `wit/torchsnap-gadget.wit` changes in a way
  that invalidates the existing guest bindings
- You update `wit-bindgen` in this crate's `Cargo.toml`

Tests that fail with a cryptic "WIT mismatch" error typically mean the
committed `.wasm` was built against an older WIT world.

## How to rebuild

From the repository root:

```sh
just build-test-fixtures
```

This recipe compiles the crate against `wasm32-wasip2`, copies the
artifact to `minimal_gadget.wasm` in this directory, and does the same
for the companion `failing-enable-gadget/` fixture.

CI does not compile fixtures — it runs the Rust test suite against the
committed `.wasm`. Rebuilds are a human responsibility gated behind
`just build-test-fixtures`, and the updated `.wasm` must be committed
alongside any change that requires it.

## Why the artifact is committed

Committing the `.wasm` means the Rust test suite has no `wasm32-wasip2`
toolchain dependency — `cargo test -p torchsnap` runs with just the host
toolchain. Building the fixture is an explicit developer action, not an
implicit test-time step.
