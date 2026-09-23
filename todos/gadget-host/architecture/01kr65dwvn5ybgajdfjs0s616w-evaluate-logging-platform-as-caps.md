# Evaluate logging and platform as capabilities

## Context

The WASM bridge exposes `logging` and `platform` host imports
(`wasm/runtime/host/logging.rs`, `wasm/runtime/host/platform.rs`) that
currently have no corresponding cap types in `caps/`. They are WASM-only
host imports without native gadget equivalents.

As part of the unified capability system, evaluate whether these should
become proper cap types:

- **Logging**: structured log output. Currently WASM-only (`LogLevel`,
  span management). Native gadgets use `eprintln!` directly. A `LoggingCap`
  could give native gadgets structured logging too.
- **Platform**: `current_os()` and `current_arch()`. Stateless compile-time
  constants. May not warrant a cap — could stay as a bridge-only concern.

## TODO

- Decide whether `LoggingCap` adds value for native gadgets or is
  unnecessary abstraction
- Decide whether `PlatformCap` is worth extracting or should remain
  bridge-only (stateless, no permission surface, no runtime state)
- If yes, create cap types in `caps/` and migrate the bridge
