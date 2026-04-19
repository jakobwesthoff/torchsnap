# Add `http` and `opener` host WIT interfaces

## Context

Two host interfaces are needed to unblock conversion of the `bangs` and
`open-url` native plugins (and future network-capable or URL-opening
plugins). Neither exists in the current WIT today.

### `opener`

Opens a URL or file path in the default OS handler (browser, Finder, etc.).
Wraps `tauri_plugin_opener::OpenerExt` — already a dependency for native
plugins.

Proposed WIT sketch:
```wit
interface opener {
    open-url(url: string) -> result<_, string>;
    reveal-path(path: string) -> result<_, string>;
}
```

`open-url` covers the browser case (bangs, open-url). `reveal-path` is
included now because `app-launcher` will need it and the host-side plumbing
is identical.

### `http`

A minimal synchronous HTTP client for simple GET/POST requests. WASM
plugins have no async runtime, so this must be a blocking host call
(the host runs it on a thread-pool or blocks a tokio task internally).

Proposed WIT sketch:
```wit
record http-request {
    url:     string,
    method:  string,           // "GET" | "POST" etc.
    headers: list<tuple<string, string>>,
    body:    option<list<u8>>,
}

record http-response {
    status:  u16,
    headers: list<tuple<string, string>>,
    body:    list<u8>,
}

interface http {
    fetch(request: http-request) -> result<http-response, string>;
}
```

Keeping it intentionally minimal: no streaming, no redirects policy knob,
no cookie jar. Plugins that need richer behaviour should request an
extension.

Security note: consider whether the host should enforce an allowlist of
domains per plugin (declared in `manifest.toml`) to prevent WASM plugins
from making arbitrary outbound requests.

## Current State

- `wit/torchsnap-plugin.wit` — no `http` or `opener` interface today
- `src-tauri/src/wasm/host/` — host implementations for `logging`,
  `settings`, `sql`, `clipboard`
- `tauri_plugin_opener` already in `src-tauri/Cargo.toml`
- `ureq` (or similar) already used by native plugins for HTTP

## Target

1. Add `opener` and `http` to `wit/torchsnap-plugin.wit` as new `import`
   interfaces inside `world torchsnap-plugin`.
2. Implement host-side handlers in `src-tauri/src/wasm/host/opener.rs` and
   `src-tauri/src/wasm/host/http.rs`.
3. Wire the new implementations into the wasmtime linker in
   `src-tauri/src/wasm/runtime.rs` (follow the pattern of existing host
   imports).
4. Regenerate WIT bindings (`just check-wit` / `wasm-tools`).
5. Expose the new interfaces in the plugin SDK
   (`plugin-sdk/` bindings if applicable).
6. Add integration tests: a small test WASM plugin that calls
   `opener::open-url` and `http::fetch` against a local mock.

## Open Questions

- Domain allowlist in `manifest.toml`: opt-in per-plugin or trust-all for
  now?
- Should `http::fetch` have a configurable timeout, or hard-code a
  reasonable default (e.g. 10 s) initially?
- `reveal-path` in `opener`: include now for completeness, or defer until
  `app-launcher` conversion is underway?

## References

- `wit/torchsnap-plugin.wit` — existing WIT world to extend
- `src-tauri/src/wasm/host/` — existing host import implementations
- `src-tauri/src/wasm/runtime.rs` — linker wiring
- `plugins/calculator/` — reference WASM plugin
- Blocked-by / enables: `todos/wasm/*-convert-bangs-to-wasm.md`,
  `todos/wasm/*-convert-open-url-to-wasm.md`
