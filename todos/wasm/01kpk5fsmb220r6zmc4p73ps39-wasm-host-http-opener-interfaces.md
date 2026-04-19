# Add `http` and `opener` host WIT interfaces

## Context

Two host interfaces are needed to unblock conversion of the `bangs` and
`open-url` native plugins (and future network-capable or URL-opening
plugins). Neither exists in the current WIT today.

### `opener`

Opens a URL in the default OS handler (browser, etc.).
Wraps `tauri_plugin_opener::OpenerExt` — already a dependency for native
plugins.

Proposed WIT sketch:
```wit
interface opener {
    open-url(url: string) -> result<_, string>;
}
```

`open-url` covers the browser case (bangs, open-url). A `reveal-path`
function (show a path in Finder/Explorer) is tracked separately and deferred
until the `app-launcher` conversion is underway.

Plugins declare which URL schemes they need under `[permissions.opener]` in
`manifest.toml`. The host enforces this at call time — any scheme not listed
returns `err("scheme not permitted: {scheme}")`. Omitting `[permissions.opener]`
entirely means the plugin has no `open-url` access (deny by default).

```toml
[permissions.opener]
schemes = ["https", "http"]
```

Validation at manifest parse time: reject an `[permissions.opener]` table
with an empty `schemes` list (declaring the section without granting anything
is a manifest authoring error). Individual scheme strings are accepted as-is
— no hardcoded allowlist — so future schemes (`ssh`, custom app protocols)
work without a manifest format change.

### `http`

A minimal synchronous HTTP client for simple GET/POST requests. WASM
plugins have no async runtime, so this must be a blocking host call
(the host runs it on a thread-pool or blocks a tokio task internally).

Proposed WIT:
```wit
variant http-method {
    get,
    post,
    put,
    patch,
    delete,
    head,
    other(string),
}

record http-request {
    url:           string,
    method:        http-method,
    headers:       list<tuple<string, string>>,
    body:          option<list<u8>>,
    timeout-ms:    option<u32>,   // none = host default
    max-body-size: option<u64>,   // none = host default; guards against unbounded downloads
}

record http-response {
    status:  u16,
    headers: list<tuple<string, string>>,
    body:    list<u8>,
}

variant http-error {
    permission-denied(string),  // origin not in manifest allowlist
    network(string),            // connection-level failure (DNS, TLS, refused, etc.)
    timeout,
}

interface http {
    fetch: func(request: http-request) -> result<http-response, http-error>;
}
```

Design notes:
- `http-method::other(string)` covers non-standard methods (WebDAV etc.) without
  opening the common cases to typos.
- `list<tuple<string, string>>` for headers maps 1:1 to `Vec<(String, String)>` in
  the existing `network::Http` abstraction — no conversion overhead.
- `max-body-size` exposes `RequestBuilder::max_size` which already exists in the
  host abstraction; `none` delegates to the host default.
- No streaming, no redirects policy knob, no cookie jar. Plugins that need richer
  behaviour should request an extension.

Host implementation notes:
- Use a `thiserror`-derived internal error type to classify `reqwest::Error` into
  the three `http-error` variants before mapping to WIT — consistent with the
  `FetchError` pattern in `src/network/website_metadata/fetch.rs`.
- `reqwest::Error::is_timeout()` → `http-error::timeout`
- Other transport errors → `http-error::network`
- Origin check failure (pre-request) → `http-error::permission-denied`
- `http-method::other(string)` must be validated via
  `reqwest::Method::from_bytes()` before sending; an invalid method string maps
  to `http-error::network`.

Security note: follow the same `[permissions]` pattern as `opener` — plugins
declare allowed origins under `[permissions.http]` in `manifest.toml`. An
origin is `scheme + host` (e.g. `"https://api.duckduckgo.com"`). The host
enforces this at call time — requests to undeclared origins return
`http-error::permission-denied`.

The special value `"*"` opts the plugin into trust-all mode, permitting
requests to any origin:

```toml
[permissions.http]
origins = ["https://api.duckduckgo.com"]   # locked down

# or:
origins = ["*"]                             # trust-all
```

`"*"` is intentionally explicit rather than implicit — plugin authors must
opt in. Future work: when a per-user permissions UX exists, requests to
origins not in the declared list (and not covered by `"*"`) could trigger
an on-demand dialog instead of a hard rejection.

## Current State

- `wit/torchsnap-plugin.wit` — no `http` or `opener` interface today
- `src-tauri/src/wasm/host/` — host implementations for `logging`,
  `settings`, `sql`, `clipboard`
- `tauri_plugin_opener` already in `src-tauri/Cargo.toml`
- `src-tauri/src/network/http.rs` — existing `Http`/`RequestBuilder`/`HttpResponse`
  abstraction over `reqwest`; types already use owned primitives designed for the
  WIT boundary. Host impl is a thin translation layer on top.

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

## References

- `wit/torchsnap-plugin.wit` — existing WIT world to extend
- `src-tauri/src/wasm/host/` — existing host import implementations
- `src-tauri/src/wasm/runtime.rs` — linker wiring
- `plugins/calculator/` — reference WASM plugin
- Blocked-by / enables: `todos/wasm/*-convert-bangs-to-wasm.md`,
  `todos/wasm/*-convert-open-url-to-wasm.md`
