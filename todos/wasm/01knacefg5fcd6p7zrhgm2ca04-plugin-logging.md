# Plugin Logging System

Design and implement the full plugin logging system beyond the minimal `log(level, message)` WIT import already sketched.

**Strategy doc:** §5.4 (stdout/stderr redirect, logging as WASI context), §5.3 (`torchsnap:host/logging` WIT interface)

**Status:** needs discussion

**Discussion needed:**
- Structured logging: should `log()` accept a structured payload (JSON) rather than a plain string, to allow key-value fields alongside the message?
- Log namespacing: plugin ID is already part of `PluginState` and will be attached as a tracing field — is that sufficient, or do plugins need sub-component namespacing?
- Stdout/stderr redirect: `WasiCtxBuilder` custom streams are how `println!` gets captured — confirm implementation approach and test it
- Log level filtering per plugin (e.g., debug logs from a noisy plugin suppressed in production)

**Notes:** The current hello-world WIT already includes the minimal `logging` interface (§5.8). This task is about deciding whether that's sufficient long-term or needs extension, then implementing whatever is decided. The stdout→`info` / stderr→`warn` redirect via `WasiCtxBuilder` is already decided in §5.4; this task implements and tests it.
