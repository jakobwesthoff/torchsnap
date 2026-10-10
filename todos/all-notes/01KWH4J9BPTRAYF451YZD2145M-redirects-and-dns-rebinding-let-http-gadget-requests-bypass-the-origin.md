---
title: "Redirects and DNS rebinding let HTTP gadget requests bypass the origin allowlist"
kind: bug
component: gadget-host
status: needs-discussion
impact: high
tags: [security, capabilities]
---
# Redirects and DNS rebinding let HTTP gadget requests bypass the origin allowlist

The HTTP capability checks the origin allowlist only on the first URL. Redirects and DNS rebinding then reach internal hosts such as loopback, RFC1918 and cloud metadata addresses, which is an SSRF from any gadget with an HTTP grant. The same capability also lets a gadget turn off TLS validation per request, and it reads response bodies with no size limit by default. Its error classification matches substrings of URLs. The SSRF fix requires a design choice (how to handle redirects, and whether to grant private-network access), so the status is needs-discussion.

## Threat model
`HttpCap` is per-gadget. The gadget author fully controls every
request field (`url`, `method`, `headers`, `body`, `timeout_ms`,
`max_body_size`, `insecure_tls`), passed straight through WIT to
`HttpRequest` and then `HttpCap::fetch` with no host mediation
(`src-tauri/src/wasm/runtime/host/http.rs:37-49`). The manifest grants only
`origins` (`src-tauri/src/wasm/manifest/permissions/http.rs:24-29`). The origin
allowlist is therefore the *entire* security control on the capability. The attacker controls both endpoints of any exchange: their gadget code,
and the servers behind their declared origins. No third party or
victim is required.

## (a) SSRF, high: two independent bypasses of the origin allowlist
`check_origin` runs once, on `request.url`, before dispatch
(`src-tauri/src/caps/http.rs:154`).

**Redirect bypass.** Redirects are followed inside `builder.send()`
(`src-tauri/src/caps/http.rs:189`). The client is built with no `.redirect(...)`
call (`src-tauri/src/network/http.rs:169-173`), so reqwest's default applies.
Verified against pinned reqwest 0.13.2: the default is
`Policy::limited(10)` (`redirect.rs:160-163`, `client.rs:306`), which allows up to
10 hops. The redirect action (`redirect.rs:305-334`) follows any hop
the count policy accepts, rejecting only non-`http` and non-`https` schemes.
`https_only` defaults to `false` (`:278`), so the downgrade guard
(`:323`) is inert and `https` to `http` downgrades are followed. There is
no IP filtering in the default path. A gadget granted
`origins = ["https://allowed.example"]` requests
`https://allowed.example/x`. That host, which the author controls,
returns `302 Location: http://169.254.169.254/latest/meta-data/...`,
`http://127.0.0.1:<port>/...`, or `http://10.x.y.z/...`, and reqwest
follows it. The allowlist gated only hop 0.

**DNS-rebinding bypass (independent of redirects).** `check_origin`
compares the URL *host string* against the allowlist
(`src-tauri/src/caps/http.rs:210-216`). reqwest resolves that host separately at
connect time, and the allowlist says nothing about the destination
IP. A gadget declares `origins = ["https://evil.example"]`, points
`evil.example`'s A record at `127.0.0.1`, `169.254.169.254`, or an
RFC1918 host, and reaches the internal endpoint with the origin check
passing and **no redirect at all**. This is the crucial framing: the
origin allowlist is a *name* control, and full SSRF defense requires a
*destination-IP* control. The redirect bug is one instance of that
gap, and rebind is the other. Fixing redirects alone does not close
rebind, and the reverse is also true.

A minor mitigating note that does **not** reduce severity: reqwest
strips `Authorization`, `Cookie`, and `Proxy-Authorization` on cross-host
redirects (`redirect.rs:338`). Cloud-metadata and most internal
endpoints need no auth header, and the attacker sets whatever headers
they want on the final hop via a self-hosted redirector anyway.

**Severity high.** This defeats the sole control of the capability with a
self-contained, no-victim exploit. On a developer or desktop target the
highest-value reach is loopback and RFC1918: local dev servers,
localhost-bound databases, admin panels, the router at
`192.168.1.1`, other gadgets' local listeners, and Torchsnap's own local
surfaces. The `169.254.169.254` metadata angle additionally applies
on any cloud or CI host. For a determined malicious author, the HTTP
capability degrades to "connect to anything reqwest can reach."

## (b) `insecure_tls` ungated, medium
`insecure-tls: bool` is a plain per-request WIT field (`wit:286`),
passed through unchanged (`host/http.rs:46`). It selects a lazily built
client with `danger_accept_invalid_certs(true)` (`caps/http.rs:158-171`
and `network/http.rs:161-172`). No manifest field gates it, since
`HttpPermissionsDef` carries only `origins`. Any gadget with any
HTTP grant can disable certificate validation per request. The WIT
comment (`wit:275-278`) frames the legitimate use as local daemons
with self-signed certs (Docker over TLS, k3s). That is a loopback or private
scenario, which matters for the fix. Severity is medium because it silently
removes a safety property the *user* assumes is on. The real exposure
is a gadget fetching from a user-trusted host with validation off over
a hostile network, exposing user data to interception with no signal.
It compounds with (a): a redirect or rebind to an internal `https`
self-signed endpoint plus `insecure_tls` broadens reach.

## (c) Unbounded response body by default, medium
`DEFAULT_MAX_SIZE = u64::MAX` (`network/http.rs:42`). `send` resolves
`max_size = self.max_size.unwrap_or(self.default_max_size)` (`:254`),
and `HttpCap::fetch` forwards the guest's `max_body_size` only when
it is `Some` (`caps/http.rs:183-185`). When the guest omits it, the effective
limit is `u64::MAX`. The Content-Length early reject (`:295-302`)
and the streaming total check (`:391-399`) never fire, and `bytes()`
buffers the full response into host memory. This contradicts the WIT
contract, which says `max-body-size: none` "delegates to the host
default (guards against unbounded downloads)" (`wit:273-274`). The host
default is `u64::MAX`, which guards nothing. Severity is medium because it is an
OOM DoS with a one-line trigger: request a large or chunked-forever resource
and omit `max_body_size`. It crashes the whole host process and every other
gadget with it. There is a secondary amplification: `bytes()` holds the full body
in memory, so N concurrent requests cost N times the cap, even with a per-request
cap.

## (d) Error classification substring bug, low and cosmetic
`from_request_error` (`caps/http.rs:73-111`) does the right thing
first (a typed `re.is_timeout()`), then falls back to lowercasing the
entire error chain, which embeds the request URL, and substring-
matching `"tls"`, `"dns"`, `"ssl"`, `"handshake"`, `"certificate"`,
and similar. A URL like `https://tls.example.com/` can force an unrelated
failure into `TlsFailed` or `DnsFailed`. This is purely cosmetic. It only selects
which `HttpError` variant the guest receives, affects no allow or deny
decision or control flow, and discloses nothing the guest didn't
supply. Severity is low.

## Suggested fix
Two orthogonal controls are needed for SSRF. The other three are
localized. These are recommendations, not decisions. #1(A) and #2 are a real
API change that makes the client origin- and policy-aware, which it is not today, since `Http::new()` is context-free.

**1. Redirects: keep the name allowlist honest across hops.**
- (A) A custom `reqwest::redirect::Policy::custom` that re-runs the
  *same* `check_origin` against an `Arc<[String]>` of the allowlist on
  each hop's `attempt.url()`, returning `follow` or `stop`. Factor
  `check_origin` so the policy and the entry check share one function,
  so they cannot drift. Thread origins from `HttpCap` into the builder. The
  closure is `Send + Sync + 'static` (owned or `Arc` clone). Lower the
  hop limit from 10 to about 5. This keeps legitimate within-allowlist multi-hop
  flows working.
- (B) `Policy::none()`, and surface the 3xx (status and `Location`) to the
  guest, which re-issues `fetch` and re-runs `check_origin`. This is a single
  enforcement point with no duplicated logic. The cost is a guest-visible
  behaviour change (redirects to canonical hosts break) and an SDK redirect-follow helper.
- Recommendation: (A) with limit 5, provided `check_origin` is shared.
  (B) if a single choke point is preferred over redirect ergonomics.
  Either way, the wildcard `"*"` origin must **not** exempt a gadget
  from control #2.

**2. IP-range egress guard with resolve-then-pin (closes rebind and backstops redirects).**
This is the control that actually prevents
reaching internal destinations, and it runs on every connection
(the initial hop and every redirect hop). Implement a custom
`reqwest::dns::Resolve` installed via `ClientBuilder::dns_resolver`.
It resolves the host, filters the returned `SocketAddr`s against a
blocked-range table, and returns only vetted addresses (an error if none
remain). Because reqwest connects to exactly what the resolver
returns, this **pins** the checked IP to the connected IP, which eliminates
the check-then-connect TOCTOU that DNS rebinding exploits. Block by
default, for IPv4 and IPv6: loopback (`127.0.0.0/8`, `::1`), link-local
(`169.254.0.0/16`, including `169.254.169.254`, and `fe80::/10`), private
(`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), ULA (`fc00::/7`),
CGNAT (`100.64.0.0/10`), and unspecified (`0.0.0.0/8`, `::`). Unwrap
IPv4-mapped IPv6 (`::ffff:0:0/96`) and re-check it, otherwise
`::ffff:127.0.0.1` bypasses the loopback rule. Use a vetted range
table (`ipnet` or `ip_network`) or a hand-rolled list, rather than the
unstable `IpAddr::is_global()`.

**3. Private-network escape hatch (resolves the tension between #2 and
insecure_tls).** #2 and `insecure_tls` are in direct tension with the
feature's own purpose. `insecure_tls` exists for local daemons, which
live on exactly the ranges #2 blocks. Add a single explicit, install-consented manifest grant (for example `[permissions.http]
allow_private_network = true`, name TBD). It relaxes the #2 IP
guard for that gadget, and it is the prerequisite for `insecure_tls`.
It is default-deny: gadgets that need local daemons declare it, and the user
sees it at install.

**4. `insecure_tls` gate.** Gate it behind the grant from #3 (or a
dedicated `allow_insecure_tls` field). As defense in depth, restrict
its effect to non-public destinations, since disabling cert validation for
public hosts is essentially always wrong. Mirror the origins pattern:
thread the grant into `HttpPermissions` and `HttpCap`. If the grant is not present, reject
`insecure_tls` (`PermissionDenied`) rather than silently
honouring it.

**5. Body default cap.** Honour the WIT promise. When the guest omits
`max_body_size`, apply a real host default instead of `u64::MAX`, and
clamp any guest-supplied value to a hard ceiling so a gadget cannot
re-request `u64::MAX`. Recommended: 32 MiB when omitted, with a hard ceiling
of about 256 MiB. Keep the general `Http` wrapper's `u64::MAX` default only
for trusted host-internal callers (website metadata), if desired. The
gadget-facing path must always be bounded. The residual concurrency
amplification (N times the cap) is a follow-on note, not blocking.

**6. Error classification.** Replace substring matching over `Display` with
reqwest typed predicates. `is_timeout` is already used, and `is_connect`,
`is_redirect`, `is_body` or `is_decode`, `is_request` or `is_builder`, and `is_status` are available. Caveat: reqwest 0.13 exposes
no `is_dns()` or `is_tls()`, so `DnsFailed`, `ConnectionRefused`, and `TlsFailed`
cannot be reconstructed cleanly from predicates alone. If
that granularity is needed, walk `std::error::Error::source()` and
downcast to concrete types (`io::Error` kind, rustls error types)
rather than the URL-contaminated `Display` string. Otherwise, fold
the DNS and TLS ambiguity into a generic transport error. Stop matching over
strings that embed the URL. With #1 in place, a hop rejected by the
redirect policy or the resolver surfaces as `is_redirect` or a connect
error. Map those to `PermissionDenied` (or a new redirect-blocked
variant) so the guest gets an accurate signal.

## Caveats and cross-references
- Manifest parsing rejects opaque origins (`file:`, `data:`, and similar) and
  origins with a path, query, or fragment, so every stored origin is a
  scheme, host, and port.
- The website-metadata fetch path
  (`src-tauri/src/network/website_metadata/fetch.rs`) shares the same `Http` wrapper
  and fetches HTML-derived favicon URLs. It has its own SSRF todo, titled "Website-metadata lookups reach internal hosts, and the favicon protocol echoes Origin". It would
  benefit from the same #2 resolver guard. **Resolved, per the
  website-metadata consult: put the guard in the shared `Http`,** so
  both `HttpCap` and website-metadata are covered by one
  implementation. An `HttpCap`-only guard would leave the
  website-metadata path as an independent unmitigated bypass, and
  duplicating the range table invites drift. Make the block and allow
  policy a per-instance `HttpBuilder` parameter. Website-metadata
  blocks private ranges unconditionally, since it has no legitimate internal reach
  and it is gadget-invokable. `HttpCap` blocks by default and
  relaxes only for a gadget holding the `allow_private_network` grant
  from #3.
- #1(A) and #2 both require making the client origin- and policy-aware.
  That is a real API change to `Http` and `HttpBuilder`, not a local patch.

## Considered but not folded in
- Enforcing `https`-only for public origins (blocking `http://`
  egress). This is plausible hardening, but it is a separate policy decision.
- A per-gadget concurrent-request or total-in-flight-bytes budget to
  close the N-times-cap memory amplification. This is a follow-on to #5.
