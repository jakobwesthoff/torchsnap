# 38. WASM plugin HTTP API

Date: 2026-04-19

## Status

Accepted

## Context

Several plugins that are candidates for WASM conversion make outbound HTTP
requests — querying REST APIs, fetching feed metadata, looking up package
information. Native plugins call `reqwest` or `ureq` directly; WASM plugins
have no such path because they run inside a sandboxed wasmtime instance with
no network access.

The primary design tension is synchrony: the WASM guest has no async runtime,
so any I/O must either be broken into callbacks (complex, not compatible with
the WIT callback model) or bridged as a blocking host call on the host's
thread pool. The latter matches how WASM plugins already call SQL storage and
matches the synchronous `execute()`/`search()` call model everywhere else in
the plugin API.

The secondary tension is trust: HTTP is a broad capability. Unrestricted
outbound HTTP from any installed WASM plugin would be a significant expansion
of the sandbox's effective capability, so per-plugin permission declarations
are necessary.

## Decision

Add a new `interface http` host import to the WIT world. The interface
exposes a single `fetch` function:

```wit
interface http {
    variant http-method { get, post, put, patch, delete, head, other(string) }

    record http-request {
        url:           string,
        method:        http-method,
        headers:       list<tuple<string, string>>,
        body:          option<list<u8>>,
        timeout-ms:    option<u32>,
        max-body-size: option<u64>,
    }

    record http-response {
        status:  u16,
        headers: list<tuple<string, string>>,
        body:    list<u8>,
    }

    variant http-error {
        permission-denied(string),
        network(string),
        timeout,
    }

    fetch: func(request: http-request) -> result<http-response, http-error>;
}
```

### Blocking bridge via `block_in_place`

`fetch` is synchronous from the guest's perspective but executes an async
reqwest call on the host. The bridge uses `tokio::task::block_in_place` to
run the async work on the caller's thread without blocking the tokio runtime
scheduler. This avoids spawning a dedicated thread per call while keeping the
WASM guest's call-return semantics intact.

### Permission model: origin allowlist from manifest

Plugins declare which origins they may reach under `[permissions.http]` in
`manifest.toml`:

```toml
[permissions.http]
origins = ["https://api.duckduckgo.com"]

# or trust-all:
origins = ["*"]
```

The `"*"` wildcard opts the plugin into trust-all mode — any origin is
permitted. This is provided because some plugins (e.g., a general-purpose
URL preview tool) legitimately need to reach arbitrary origins. Plugin
reviewers and users can see the wildcard in the manifest and make an informed
trust decision. Omitting `[permissions.http]` entirely means the plugin has
no HTTP access (deny by default).

### Origin normalization at parse time

Declared origins are normalized via `url::Url::origin().ascii_serialization()`
when the manifest is loaded, not at call time. This means:

- `https://API.EXAMPLE.COM` and `https://api.example.com` collapse to the
  same entry.
- A manifest with `"https://api.example.com:443"` normalizes to
  `"https://api.example.com"` (default port elision).
- Invalid URLs in the `origins` list fail plugin load with a clear error.

Normalizing at parse time prevents drift between how an origin was declared
and how the request URL's origin looks after the browser's own normalization.

### Error variants: three categories, no catch-all

`http-error` has exactly three variants:

- `permission-denied(string)` — the request URL's origin is not in the
  allowlist. The string contains the blocked origin for diagnostics.
- `network(string)` — any connection-level failure: DNS resolution, TLS
  handshake, connection refused, invalid method string, or any other
  transport error. All real transport failures fit here.
- `timeout` — the request exceeded the configured or host-default timeout.
  Separated from `network` because it requires different handling in the
  plugin (retry with back-off vs. abort).

No `other` or catch-all variant is provided. Any transport failure that
doesn't clearly belong in `timeout` fits `network`. Adding a catch-all
would let the host return opaque errors that plugins can't act on.

### Request fields: `timeout-ms` and `max-body-size` are optional guards

Both fields default to host-defined values when `none`. `max-body-size`
guards against unbounded response bodies (e.g., a plugin inadvertently
fetching a large file). Plugins that need to receive large bodies can raise
the limit explicitly. `timeout-ms` lets plugins override the host default
for latency-sensitive calls.

### `http-method::other(string)`

The six common HTTP methods are named variants to prevent typos. `other(string)`
covers non-standard methods (WebDAV verbs, etc.) without growing the named
list for rare cases. The host validates `other` strings at call time; an
empty or malformed method string returns `network(string)`.

## Alternatives considered

* **Async guest interface** (callbacks or future-style) — rejected. The
  WIT Component Model has no async model stable enough for production use in
  early 2026. The blocking `block_in_place` bridge matches the synchronous
  call model used everywhere else in the plugin API.

* **Per-URL allowlist** instead of per-origin — more granular, but
  impractical for any plugin that queries parameterized URLs. Origin-level
  control is the right granularity.

* **No `"*"` wildcard** — rejected. Some legitimate plugins (URL preview,
  link checker) genuinely need unrestricted outbound access. Blocking them
  from WASM without a trust-all escape valve would force them to stay native
  forever. The wildcard is visible in the manifest so it is auditable.

* **A `catch-all` error variant** — rejected. See the "three categories, no
  catch-all" rationale above. A catch-all would make `permission-denied`
  swallowable and degrade the diagnostic signal.

* **Streaming response bodies** — rejected for the initial implementation.
  Streaming requires either a WIT resource type for the body or a callback
  interface, both of which add significant complexity. The `max-body-size`
  guard protects against large responses until streaming is worth adding.

## Consequences

* WASM plugins can make synchronous HTTP requests. Plugins like feed readers,
  API browsers, or any tool that needs to query a remote service can be
  converted to WASM.
* Native plugins continue to call `reqwest` / `ureq` directly. This
  interface is WASM-only.
* The `[permissions.http]` block in the manifest is the sole permission
  declaration. No runtime prompts; no separate capability file.
* Plugins that never declare `[permissions.http]` pay no runtime cost.
* The three-variant `http-error` gives plugins enough signal to distinguish
  permission failures (misconfigured manifest), network failures (unreachable
  host), and timeout failures (need retry logic) without an opaque catch-all.
* Origin normalization at manifest load time means a misconfigured origin
  (invalid URL) fails plugin load with a clear error rather than silently
  blocking all requests at runtime.
* Streaming response bodies are not supported. Plugins that must process
  large streaming responses (e.g., server-sent events) cannot use this
  interface and must stay native.
