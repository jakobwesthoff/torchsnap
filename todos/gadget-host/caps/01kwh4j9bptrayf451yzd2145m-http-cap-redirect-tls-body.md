---
kind: bug
severity: high
status: open
area: [src-tauri/src/caps/http.rs, src-tauri/src/network/http.rs]
tags: [security]
---

# HTTP capability: redirect + DNS-rebind SSRF bypass the origin allowlist, guest-toggled TLS bypass, unbounded default body, fragile error classification

## Threat model
`HttpCap` is per-gadget. The gadget author fully controls every
request field (`url`, `method`, `headers`, `body`, `timeout_ms`,
`max_body_size`, `insecure_tls`), passed straight through WIT →
`HttpRequest` → `HttpCap::fetch` with no host mediation
(`wasm/runtime/host/http.rs:37-49`). The manifest grants only
`origins` (`wasm/manifest/permissions/http.rs:24-29`). So the origin
allowlist is the *entire* security control on the capability, and the
attacker controls both endpoints of any exchange (their gadget code
and the servers behind their declared origins). No third party or
victim is required.

## (a) SSRF — HIGH — two independent bypasses of the origin allowlist
`check_origin` runs once, on `request.url`, before dispatch
(`caps/http.rs:154`).

**Redirect bypass.** Redirects are followed inside `builder.send()`
(`caps/http.rs:189`); the client is built with no `.redirect(...)`
call (`network/http.rs:169-173`), so reqwest's default applies.
Verified against pinned reqwest 0.13.2: default is
`Policy::limited(10)` (`redirect.rs:160-163`, `client.rs:306`) — up to
10 hops. The redirect action (`redirect.rs:305-334`) follows any hop
the count-policy accepts, rejecting only non-`http`/`https` schemes;
`https_only` defaults `false` (`:278`), so the downgrade guard
(`:323`) is inert and `https → http` downgrades are followed. There is
no IP filtering in the default path. So a gadget granted
`origins = ["https://allowed.example"]` requests
`https://allowed.example/x`; that host (which the author controls)
returns `302 Location: http://169.254.169.254/latest/meta-data/…` or
`http://127.0.0.1:<port>/…` or `http://10.x.y.z/…`, and reqwest
follows it. The allowlist gated only hop 0.

**DNS-rebinding bypass (independent of redirects).** `check_origin`
compares the URL *host string* against the allowlist
(`caps/http.rs:210-216`); reqwest resolves that host separately at
connect time, and the allowlist says nothing about the destination
IP. A gadget declares `origins = ["https://evil.example"]`, points
`evil.example`'s A record at `127.0.0.1` / `169.254.169.254` / an
RFC1918 host, and reaches the internal endpoint with the origin check
passing and **no redirect at all**. This is the crucial framing: the
origin allowlist is a *name* control; full SSRF defense requires a
*destination-IP* control. The redirect bug is one instance of that
gap; rebind is the other. Fixing redirects alone does not close
rebind, and vice-versa.

Minor mitigating note that does **not** reduce severity: reqwest
strips `Authorization`/`Cookie`/`Proxy-Authorization` on cross-host
redirect (`redirect.rs:338`). Cloud-metadata and most internal
endpoints need no auth header, and the attacker sets whatever headers
they want on the final hop via a self-hosted redirector anyway.

**Severity HIGH:** defeats the sole control of the capability with a
self-contained, no-victim exploit. On a developer/desktop target the
highest-value reach is loopback and RFC1918 — local dev servers,
localhost-bound databases, admin panels, the router at
`192.168.1.1`, other gadgets' local listeners, Torchsnap's own local
surfaces. The `169.254.169.254` metadata angle additionally applies
on any cloud/CI host. For a determined malicious author the HTTP
capability degrades to "connect to anything reqwest can reach."

## (b) `insecure_tls` ungated — MEDIUM
`insecure-tls: bool` is a plain per-request WIT field (`wit:286`),
passed through unchanged (`host/http.rs:46`), selecting a lazily-built
client with `danger_accept_invalid_certs(true)` (`caps/http.rs:158-171`
→ `network/http.rs:161-172`). No manifest field gates it
(`HttpPermissionsDef` carries only `origins`). Any gadget with any
HTTP grant can disable certificate validation per request. The WIT
comment (`wit:275-278`) frames the legitimate use as local daemons
with self-signed certs (Docker over TLS, k3s) — a loopback/private
scenario, which matters for the fix. Severity MEDIUM: it silently
removes a safety property the *user* assumes is on; the real exposure
is a gadget fetching from a user-trusted host with validation off over
a hostile network, exposing user data to interception with no signal.
Compounds with (a) — a redirect/rebind to an internal `https`
self-signed endpoint plus `insecure_tls` broadens reach.

## (c) Unbounded response body by default — MEDIUM
`DEFAULT_MAX_SIZE = u64::MAX` (`network/http.rs:42`). `send` resolves
`max_size = self.max_size.unwrap_or(self.default_max_size)` (`:254`),
and `HttpCap::fetch` forwards the guest's `max_body_size` only when
`Some` (`caps/http.rs:183-185`). When the guest omits it the effective
limit is `u64::MAX`, so the Content-Length early-reject (`:295-302`)
and the streaming total check (`:391-399`) never fire and `bytes()`
buffers the full response into host memory. This contradicts the WIT
contract, which says `max-body-size: none` "delegates to the host
default (guards against unbounded downloads)" (`wit:273-274`) — the
host default is `u64::MAX`, which guards nothing. Severity MEDIUM:
OOM DoS, one-line trigger (request a large/chunked-forever resource,
omit `max_body_size`); crashes the whole host process and every other
gadget with it. Secondary amplification: `bytes()` holds the full body
in memory, so N concurrent requests cost N×cap even with a per-request
cap.

## (d) Error classification substring bug — LOW / cosmetic
`from_request_error` (`caps/http.rs:73-111`) does the right thing
first (typed `re.is_timeout()`), then falls back to lowercasing the
entire error chain (which embeds the request URL) and substring-
matching `"tls"`, `"dns"`, `"ssl"`, `"handshake"`, `"certificate"`,
etc. A URL like `https://tls.example.com/` can force an unrelated
failure into `TlsFailed`/`DnsFailed`. Purely cosmetic: it only selects
which `HttpError` variant the guest receives, affects no allow/deny
decision or control flow, and discloses nothing the guest didn't
supply. Severity LOW.

## Suggested fix
Two orthogonal controls are needed for SSRF; the other three are
localized. (Recommendations, not decisions — #1(A) and #2 are a real
API change making the client origin-/policy-aware, which today it is
not: `Http::new()` is context-free.)

**1. Redirects — keep the name allowlist honest across hops.**
- (A) Custom `reqwest::redirect::Policy::custom` that re-runs the
  *same* `check_origin` against an `Arc<[String]>` of the allowlist on
  each hop's `attempt.url()`, returning `follow`/`stop`. Factor
  `check_origin` so the policy and the entry check share one function
  (no drift). Thread origins from `HttpCap` into the builder; the
  closure is `Send + Sync + 'static` (owned/`Arc` clone). Lower the
  hop limit 10 → ~5. Keeps legitimate within-allowlist multi-hop
  flows working.
- (B) `Policy::none()` + surface the 3xx (status + `Location`) to the
  guest, which re-issues `fetch` and re-runs `check_origin`. Single
  enforcement point, no duplicated logic; but a guest-visible
  behaviour change (bangs and any API that 301s to a canonical host
  break) and an SDK redirect-follow helper is needed.
- Recommendation: (A) with limit 5, provided `check_origin` is shared;
  (B) if a single choke point is preferred over redirect ergonomics.
  Either way the wildcard `"*"` origin must **not** exempt a gadget
  from control #2.

**2. IP-range egress guard with resolve-then-pin (closes rebind,
backstops redirects).** This is the control that actually prevents
reaching internal destinations, and it runs on every connection
(initial hop and every redirect hop). Implement a custom
`reqwest::dns::Resolve` installed via `ClientBuilder::dns_resolver`:
resolve the host, filter the returned `SocketAddr`s against a
blocked-range table, and return only vetted addresses (error if none
remain). Because reqwest connects to exactly what the resolver
returns, this **pins** the checked IP to the connected IP, eliminating
the check-then-connect TOCTOU that DNS rebinding exploits. Block by
default (IPv4 + IPv6): loopback (`127.0.0.0/8`, `::1`), link-local
(`169.254.0.0/16` incl. `169.254.169.254`, `fe80::/10`), private
(`10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`), ULA (`fc00::/7`),
CGNAT (`100.64.0.0/10`), unspecified (`0.0.0.0/8`, `::`). Unwrap
IPv4-mapped IPv6 (`::ffff:0:0/96`) and re-check, otherwise
`::ffff:127.0.0.1` bypasses the loopback rule. Use a vetted range
table (`ipnet`/`ip_network`) or a hand-rolled list rather than the
unstable `IpAddr::is_global()`.

**3. Private-network escape hatch (resolves the tension between #2 and
insecure_tls).** #2 and `insecure_tls` are in direct tension with the
feature's own purpose: `insecure_tls` exists for local daemons, which
live on exactly the ranges #2 blocks. Add a single explicit,
install-consented manifest grant (e.g. `[permissions.http]
allow_private_network = true`, name TBD) that (a) relaxes the #2 IP
guard for that gadget and (b) is the prerequisite for `insecure_tls`.
Default-deny; gadgets that need local daemons declare it and the user
sees it at install.

**4. `insecure_tls` gate.** Gate behind the grant from #3 (or a
dedicated `allow_insecure_tls` field) and, defense-in-depth, restrict
its effect to non-public destinations (disabling cert validation for
public hosts is essentially always wrong). Mirror the origins pattern:
thread the grant into `HttpPermissions`/`HttpCap`; if not granted,
reject `insecure_tls` (`PermissionDenied`) rather than silently
honouring it.

**5. Body default cap.** Honour the WIT promise: when the guest omits
`max_body_size`, apply a real host default instead of `u64::MAX`, and
clamp any guest-supplied value to a hard ceiling so a gadget cannot
re-request `u64::MAX`. Recommended: 32 MiB when omitted, hard ceiling
~256 MiB. Keep the general `Http` wrapper's `u64::MAX` default only
for trusted host-internal callers (website-metadata) if desired; the
gadget-facing path must always be bounded. Residual concurrency
amplification (N×cap) is a follow-on note, not blocking.

**6. Error classification.** Replace substring-over-`Display` with
reqwest typed predicates (`is_timeout` already used, plus
`is_connect`, `is_redirect`, `is_body`/`is_decode`,
`is_request`/`is_builder`, `is_status`). Caveat: reqwest 0.13 exposes
no `is_dns()`/`is_tls()`, so `DnsFailed` vs `ConnectionRefused` vs
`TlsFailed` cannot be reconstructed cleanly from predicates alone; if
that granularity is needed, walk `std::error::Error::source()` and
downcast to concrete types (`io::Error` kind, rustls error types)
rather than the URL-contaminated `Display` string; otherwise fold
DNS/TLS ambiguity into a generic transport error. Stop matching over
strings that embed the URL. With #1 in place, a hop rejected by the
redirect policy or the resolver surfaces as `is_redirect`/a connect
error — map those to `PermissionDenied` (or a new redirect-blocked
variant) so the guest gets an accurate signal.

## Caveats / cross-references
- Opaque-origin and path-dropping normalization gaps are tracked in
  `../host-wasm/01kwfz4kkaq7spwnm2ncket1ga-http-origin-opaque-and-path.md`;
  #2's IP guard is orthogonal and does not fix those.
- The website-metadata fetch path
  (`network/website_metadata/fetch.rs`) shares the same `Http` wrapper
  and fetches HTML-derived favicon URLs (its own SSRF finding:
  `01kwh4j9bptrayf451yzd2145p-website-metadata-ssrf-cors.md`); it would
  benefit from the same #2 resolver guard. **Resolved (per the
  website-metadata consult): put the guard in the shared `Http`, so
  both `HttpCap` and website-metadata are covered by one
  implementation** — a `HttpCap`-only guard would leave the
  website-metadata path as an independent unmitigated bypass, and
  duplicating the range table invites drift. Make the block/allow
  policy a per-instance `HttpBuilder` parameter: website-metadata
  blocks private ranges unconditionally (no legitimate internal reach,
  and it is gadget-invokable), while `HttpCap` blocks by default and
  relaxes only for a gadget holding the `allow_private_network` grant
  from #3.
- #1(A) and #2 both require making the client origin-/policy-aware —
  a real API change to `Http`/`HttpBuilder`, not a local patch.

## Considered but not folded in
- Enforcing `https`-only for public origins (blocking `http://`
  egress): plausible hardening, separate policy decision.
- A per-gadget concurrent-request / total-in-flight-bytes budget to
  close the N×cap memory amplification: follow-on to #5.
