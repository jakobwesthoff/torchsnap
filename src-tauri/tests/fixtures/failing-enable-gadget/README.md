# Failing-Enable Test Plugin Fixture

Test-only WASM plugin whose guest `lifecycle::enable` panics. Used by
the bridge test suite in `src-tauri/src/wasm/bridge.rs` to drive the
"guest enable() failed → bridge drops the instance" path (see ADR 0033).

## How the failure surfaces

A Rust `panic!` inside a guest export becomes a wasmtime trap on the
host. `WasmPluginInstance::enable()` returns that trap as an
`anyhow::Error`, and the bridge's `Plugin::enable` implementation uses
that signal to tear the just-created instance back down to `None`.

Every other guest export in this fixture is a no-op copy of the
minimal plugin — the fixture is only interesting during `enable()`.

## Rebuilding

This fixture is rebuilt by the same recipe as the minimal plugin:

```sh
just build-test-fixtures
```

See `../minimal-plugin/README.md` for details on when fixtures must be
rebuilt and why the `.wasm` artifact is committed rather than compiled
by CI.
