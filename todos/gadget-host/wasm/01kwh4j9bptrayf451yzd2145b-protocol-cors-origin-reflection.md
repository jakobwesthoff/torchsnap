---
kind: improvement
severity: low
status: open
area: [src-tauri/src/wasm/protocol.rs]
tags: [security]
---

# Gadget asset protocol reflects any request Origin into `Access-Control-Allow-Origin`

## Problem
`handle_request` reads the request's `Origin` header and echoes it
verbatim into `Access-Control-Allow-Origin`, falling back to `*` when
the header is absent (`src-tauri/src/wasm/protocol.rs:83-100`):

```rust
let origin = request.headers().get("Origin")
    .and_then(|v| v.to_str().ok())
    .unwrap_or("*");
// ... both the 200 and the error response set:
.header("Access-Control-Allow-Origin", origin)
```

Reflecting the Origin makes *every* origin an allowed CORS reader of
the response. The handler serves the bytes of any registered gadget's
frontend files (manifest, `.wasm`, JS/CSS/images) by id
(`serve_gadget_asset`, `:112-154`).

The legitimate consumers of the scheme are a fixed, small set:

- Dynamic `import()` of gadget bundles (`src/gadgets/wasmPluginLoader.ts:40`)
  and `fetch()` of gadget CSS (`src/lib/gadgetCss.ts:25`) — both send an
  `Origin` header.
- `<img src="torchsnap-gadget://…">` for gadget asset icons
  (`src/components/Icon.tsx:74`) — a no-cors image load that sends **no**
  `Origin` header, so it currently exercises the `*` fallback.

Every host window is created with `WebviewUrl::App`
(`src-tauri/src/lib.rs:213,958`); there is no `WebviewUrl::External`
and no `on_navigation` guard anywhere in `src-tauri/src/`, so the only
origins that can reach the scheme are `tauri://localhost` (macOS/Linux
prod), `https://tauri.localhost` (Windows prod), and the dev server
`http://localhost:1420`.

## Impact
For the reflection to matter, attacker-controlled script must run at a
non-app origin inside one of these webviews. That is not
architecturally excluded — `tauri.conf.json` sets `csp: null` (no CSP,
see the CSP todo) and there is no navigation guard — but the only thing
an attacker origin gains is the ability to read gadget asset bytes,
which is static plugin code rather than user data or credentials. No
`Access-Control-Allow-Credentials` header is set, so reflection is
functionally equivalent to `*`: neither shares credentialed responses.
This is therefore a hardening item, not a live data-disclosure bug.

Two facts bound what a fix can achieve:

- CORS is not an inter-gadget boundary here. Gadget frontend bundles are
  `import()`ed directly into the host webview and run at the host origin
  with full IPC access (`src/gadgets/wasmPluginLoader.ts`). Every gadget
  frontend can already read every other gadget's assets same-origin, so
  per-gadget `Access-Control-Allow-Origin` gating would buy nothing. The
  underlying fact — gadget frontend JS is fully-trusted host-origin code
  with full IPC — is an architectural property worth recording in its
  own right (it is what makes the null CSP and the broad asset-protocol
  scope matter; see the CSP/asset and gadget-CSS todos). B1 does not fix
  it; per-gadget origin isolation would require a different loading model
  (e.g. iframing gadget UIs).
- The `*` fallback is exercised by legitimate traffic (the `<img>` icon
  loads above), so the fix must keep serving Origin-less requests.

## Suggested fix
In `handle_request` replace reflection with a fixed allowlist:

- `Origin` present and in { `tauri://localhost`, `https://tauri.localhost`,
  plus `http://localhost:1420` under `cfg(debug_assertions)` } → echo
  that origin and add `Vary: Origin`.
- `Origin` present but not allowlisted → `403` with no ACAO header.
- `Origin` absent → serve `200` with **no** ACAO header (do not emit
  `*`); this keeps the no-cors `<img>` icon loads working.

Apply the same logic to the error-response branch (`:96-101`), which
reflects too. Convert the `cors_header_echoed` test (`:343`) into
allowlist cases: allowlisted origin echoed with `Vary`, disallowed
origin `403`, absent origin `200` without ACAO. GET module/`fetch`
requests do not preflight, so no `OPTIONS` handling is required.

Adjacent hardening that determines whether this class of issue is
reachable at all: the null CSP and the absent `on_navigation` guard
(tracked in the CSP/assetProtocol todo).
