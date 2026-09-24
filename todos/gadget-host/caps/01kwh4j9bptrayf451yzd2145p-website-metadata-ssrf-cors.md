---
kind: bug
severity: medium
status: open
area: [src-tauri/src/network/website_metadata/fetch.rs, src-tauri/src/network/website_metadata/mod.rs, src-tauri/src/network/website_metadata/protocol.rs]
tags: [security]
---

# Website-metadata service is a gadget-driven SSRF (reachability oracle + redirect amplification + internal-HTML read); favicon protocol echoes Origin and spawns a thread per request

## Invocation surface — gadget-driven (decisive for severity)
The website-metadata service is exposed to untrusted WASM gadgets as a
host capability; it is **not** limited to user-typed URLs or a single
built-in gadget:
- WIT: the `website-metadata` interface exports
  `lookup: func(domain: string, mode: lookup-mode)` imported by the
  `gadget` world (`torchsnap-gadget.wit:858-904,921`), gated by
  manifest `[permissions] website-metadata = true` (`:856-857`).
- Host shim `GadgetState::lookup`
  (`wasm/runtime/host/website_metadata.rs:22-45`) reads
  `self.caps.website_metadata` (present iff granted) and forwards
  `domain`/`mode` to `WebsiteMetadataCap::lookup`
  (`caps/website_metadata.rs:36-48`) →
  `WebsiteMetadataService::lookup` (`mod.rs:372-402`) →
  `fetch_and_cache_inner` (`:588`) → the fetch functions.
- The only callers of `lookup` are the WIT shim and tests; the two
  Tauri commands touching the service (`lib.rs:524-540`) are
  `website_metadata_stats` / `website_metadata_clear_cache`, never
  `lookup`. Benign consumers are themselves gadgets (`open-url` calls
  `lookup_blocking(&detected.domain)`, `bangs` calls `favicon_or(...)`).

So any installed gadget granted `website-metadata` invokes `lookup`
with a `domain` **of its choosing**, and `Blocking` mode runs the
fetch synchronously on demand. The one constraint is `validate_domain`
(`mod.rs:739-765`), which rejects `://`, `/`, `:`, whitespace, and
leading/trailing dots — so no full URL, port, or IPv6 literal. But a
**bare IPv4 literal or internal hostname passes** (`169.254.169.254`,
`192.168.1.1`, `127.0.0.1`, `localhost`, `metadata.google.internal`),
and production `url_for_path` (`mod.rs:284`) builds `https://{domain}/`.

## SSRF reach
The service's `Http` client (`mod.rs:253-257`) is built via
`Http::builder()` with no `dns_resolver`, no redirect policy, and no
scheme restriction — the same wrapper type as the HTTP cap (B13), with
no IP filtering on either.

- **(a) Page fetch — gadget picks the host; reachability oracle +
  partial read.** `fetch_page_metadata` (`fetch.rs:80-88`) GETs
  `https://{domain}/`. Two signals flow back to the gadget via
  `LookupResult`: a **reachability oracle** (`Unreachable` vs
  `ReachableNoData` vs `Hit`, `mod.rs:620-651`) lets a gadget
  port/host-scan internal **HTTPS** services by iterating bare hosts —
  no victim, no attacker page; and a **partial read** — an internal
  HTML endpoint's `<title>` / `<meta description>` / OG tags are
  extracted (`metadata.rs:65-77`) and returned to the gadget in
  `CacheEntry.title/description` (`host/website_metadata.rs:65-69`), a
  narrow internal-content read.
- **(b) Redirect amplification — https-forced page fetch → arbitrary
  http SSRF.** No redirect policy is set, so reqwest's default applies
  (`Policy::limited(10)`, follows `https→http` downgrades — validated
  in B13 against reqwest 0.13.2). A gadget passes `domain =
  attacker.example`; the attacker's `https://attacker.example/` returns
  `302 Location: http://169.254.169.254/latest/meta-data/…` (or
  `http://127.0.0.1:<port>/…`, `http://10.x.y.z/…`), and the host
  follows it. `https://{domain}/` constrains only hop 0; redirects
  reach arbitrary internal **http** targets and their reachability /
  HTML-metadata surfaces per (a).
- **(c) Favicon fetch — page-controlled URL, blind GET.**
  `fetch_favicon_image` (`fetch.rs:167-240`) GETs `favicon_url`, which
  is resolved from `<link rel="icon" href="…">` via `base_url.join`
  (`metadata.rs:300`); an absolute `http://169.254.169.254/…` href is
  preserved. Since the gadget controls the page (its own domain, or via
  the redirect in (b)), it controls the favicon URL: any http/https
  host/port/path (reqwest rejects non-http(s)). Mitigations are only
  2s timeout + 256 KB cap. This path is **blind to the gadget**: the
  response is stored to disk (`FaviconStore`) and returned as an
  `EntryIcon::AssetIcon(url)` string, not bytes, so the guest never
  reads the favicon body — at most it learns "image-shaped" (AssetIcon
  vs `globe-alt` HeroIcon). Blind internal GET (side effects, port
  probing), not a read primitive.
- **(d) `data:` inline.** `decode_data_uri` (`metadata.rs:95-137`)
  accepts only `image/*`, decodes inline (no network), and stores the
  bytes. A gadget can store arbitrary attacker bytes labelled as an
  image, including SVG, served via `torchsnap-favicon://` and rendered
  in an `<img>` (non-scripted context), so embedded SVG script does not
  execute. Trust-chain risk here is low; confirmed.

**Net:** gadget-driven SSRF with (i) a direct internal-HTTPS
reachability oracle from the `domain` argument, (ii) redirect-based
reach to arbitrary internal http endpoints, (iii) a narrow read of
internal HTML `<title>`/description/OG, and (iv) a blind favicon GET to
any internal http(s) URL. It is a notch below B13's high because the
gadget cannot set method/headers/body and cannot read arbitrary
response bodies (only extracted metadata fields + a reachable/
image-shaped bit; the favicon path is fully blind) — but it shares
B13's exact root cause and is reached through a distinct,
install-consented capability. Severity: **medium (upper)**.

## (CORS) Origin echo — LOW
`handle_request` (`protocol.rs:109-131`) reflects the request `Origin`
into `Access-Control-Allow-Origin`, fallback `*`. No
`Access-Control-Allow-Credentials`, so reflection is equivalent to
`*`. The scheme serves favicon image bytes to the host webview via
`<img>` (no Origin → the `*` fallback is normal traffic). Favicon bytes
are not secrets, so the incremental read risk is negligible. Same
pattern already fixed on the gadget asset scheme with an origin
allowlist; apply the identical fix here for consistency, not because
this scheme leaks anything sensitive.

## (Thread) One OS thread per request — LOW
`register_favicon_protocol` (`protocol.rs:88-97`) does
`std::thread::spawn` per request to run the blocking
`FaviconStore::resolve` + `std::fs::read`. Unbounded short-lived OS
threads under a request burst.

## Suggested fix

### SSRF (primary)
1. **Apply B13's `reqwest::dns::Resolve` resolve-then-pin egress
   guard, in the shared `Http`/`HttpBuilder`** so both `HttpCap` and
   website-metadata are covered by one implementation. This is the
   concrete resolution of B13's parked "where does the guard live"
   question: shared `Http`, because a `HttpCap`-only guard leaves this
   path as an independent unmitigated bypass and duplicating the range
   table invites drift. The guard resolves the host, filters returned
   `SocketAddr`s against the blocked-range table (loopback, link-local
   incl. `169.254.169.254`, RFC1918, ULA, CGNAT, unspecified; v4+v6;
   unwrap IPv4-mapped v6), and returns only vetted addresses. Because
   reqwest connects to exactly what the resolver returns, the checked
   IP is pinned to the connected IP — this **automatically covers every
   redirect hop** (each hop reconnects through the resolver), closing
   both the redirect and DNS-rebind vectors for website-metadata
   without a separate redirect policy. website-metadata has no origin
   allowlist to keep honest across hops (unlike `HttpCap`), so the
   resolver guard is the primary and sufficient control here; lowering
   the default hop limit is optional hardening.
2. **Default-deny private ranges for website-metadata, no escape
   hatch.** Make the guard policy a per-instance `HttpBuilder`
   parameter. Is B13's `allow_private_network` grant relevant here?
   No: the service fetches metadata for public registrable domains for
   search results; there is no legitimate reason for it to reach
   internal/private IPs, and it is gadget-invokable, so the SSRF risk
   dominates any local-dev-preview nicety. Construct the
   website-metadata `Http` with private ranges blocked unconditionally;
   only `HttpCap` instances whose gadget holds `allow_private_network`
   opt into relaxation. Shared code, parameterized by policy; the two
   callers pass different policies.
3. **Restrict fetch schemes to http/https explicitly** on both
   `fetch_page_metadata` and `fetch_favicon_image` (reqwest already
   rejects other schemes, but an explicit check makes the contract
   clear and rejects early). `data:` stays handled inline before the
   network call, image-mime-gated as today.
4. **Local-dev preview tradeoff (state as a decision):** the page
   fetch is gadget-driven, not user-typed, so the "developer previewing
   `localhost`" case is weak justification. Default-deny private
   ranges; if a local-preview affordance is ever wanted, gate it behind
   an explicit separate opt-in rather than leaving the range open.

### CORS
Replace the Origin echo with the same origin allowlist used on the
gadget asset scheme
(`../host-wasm/01kwh4j9bptrayf451yzd2145b-protocol-cors-origin-reflection.md`);
factor the allowlist so both schemes share it.

### Thread-per-request
Replace `std::thread::spawn` with `tauri::async_runtime::spawn_blocking`
(bounded pool), per
`../host-wasm/01kwfz4kkaq7spwnm2ncket1ft-protocol-thread-per-request.md`.

## Cross-references
- B13 (`01kwh4j9bptrayf451yzd2145m-http-cap-redirect-tls-body.md`) —
  shared root cause and the egress-guard spec; the guard-placement
  decision above (shared `Http`) resolves B13's parked caveat.
- Gadget-scheme CORS fix and thread-per-request todo — mirror both.
