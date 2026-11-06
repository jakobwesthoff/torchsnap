---
title: "Gate insecure_tls behind a manifest grant for HTTP gadget requests"
kind: bug
component: gadget-host
tags: [security, capabilities]
---
# Gate insecure_tls behind a manifest grant for HTTP gadget requests

Any gadget with any HTTP grant can turn off TLS certificate validation per request, because `insecure_tls` is not gated by the manifest. Found in a review of the HTTP capability (`HttpCap`, per-gadget); the review's date is not recorded in the original todo. The gadget author controls every request field, and the manifest grants only `origins`, so nothing stops the author from setting the flag.

## Problem
`insecure-tls: bool` is a plain per-request WIT field (`wit:286`),
passed through unchanged (`src-tauri/src/wasm/runtime/host/http.rs:46`). It selects a lazily built
client with `danger_accept_invalid_certs(true)` (`src-tauri/src/caps/http.rs:158-171`
and `src-tauri/src/network/http.rs:161-172`). No manifest field gates it, since
`HttpPermissionsDef` (`src-tauri/src/wasm/manifest/permissions/http.rs:24-29`)
carries only `origins`. The WIT
comment (`wit:275-278`) frames the legitimate use as local daemons
with self-signed certs (Docker over TLS, k3s). That is a loopback or private
scenario, which matters for the fix.

## Impact
Medium. It silently removes a safety property the *user* assumes is on.
The real exposure is a gadget fetching from a user-trusted host with
validation off over a hostile network, exposing user data to interception
with no signal. It compounds with the SSRF todo: a redirect or rebind to
an internal `https` self-signed endpoint plus `insecure_tls` broadens
reach.

## Suggested fix
Gate `insecure_tls` behind a manifest grant: either the
`allow_private_network` grant proposed in the SSRF todo, or a dedicated
`allow_insecure_tls` field. As defense in depth, restrict its effect to
non-public destinations, since disabling cert validation for public hosts
is essentially always wrong. Mirror the origins pattern: thread the grant
into `HttpPermissions` and `HttpCap`. If the grant is not present, reject
`insecure_tls` (`PermissionDenied`) rather than silently honouring it.
The dedicated field works without waiting for the SSRF decision.

## Relations

- Relates to: [Redirects and DNS rebinding let HTTP gadget requests bypass the origin allowlist](01KWH4J9BPTRAYF451YZD2145M-redirects-and-dns-rebinding-let-http-gadget-requests-bypass-the-origin.md), the `allow_private_network` grant is a candidate gate and the combination broadens reach
