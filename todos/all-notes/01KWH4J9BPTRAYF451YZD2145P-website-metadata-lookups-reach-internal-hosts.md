---
title: "Website-metadata lookups reach internal hosts"
kind: bug
component: backend
tags: [security]
---
# Website-metadata lookups reach internal hosts

Installed gadgets with the `website-metadata` permission choose the domain that the host fetches. The host follows redirects and fetches favicon URLs taken from page HTML with no IP filtering, which makes the service a gadget-driven SSRF. It can probe internal HTTPS hosts, read their titles and descriptions, and blindly request internal http and https URLs. The SSRF fix is shared with the HTTP capability todo.

## Invocation surface: gadget-driven (decisive for severity)
The website-metadata service is exposed to untrusted WASM gadgets as a
host capability. It is **not** limited to user-typed URLs or a single
built-in gadget:
- WIT: the `website-metadata` interface exports
  `lookup: func(domain: string, mode: lookup-mode)`, imported by the
  `gadget` world (`gadgets/gadget-sdk/wit/torchsnap-gadget.wit:858-904,921`). It is gated by
  manifest `[permissions] website-metadata = true` (`:856-857`).
- The host shim `GadgetState::lookup`
  (`src-tauri/src/wasm/runtime/host/website_metadata.rs:22-45`) reads
  `self.caps.website_metadata` (present if and only if granted) and forwards
  `domain` and `mode` to `WebsiteMetadataCap::lookup`
  (`src-tauri/src/caps/website_metadata.rs:36-48`), then to
  `WebsiteMetadataService::lookup` (`src-tauri/src/network/website_metadata/mod.rs:372-402`),
  then to `fetch_and_cache_inner` (`:588`) and the fetch functions.
- The only callers of `lookup` are the WIT shim and tests. The two
  Tauri commands that touch the service (`src-tauri/src/lib.rs:524-540`) are
  `website_metadata_stats` and `website_metadata_clear_cache`, never
  `lookup`. The benign consumers are themselves gadgets (`open-url` calls
  `lookup_blocking(&detected.domain)`, and `bangs` calls `favicon_or(...)`).

So any installed gadget granted `website-metadata` invokes `lookup`
with a `domain` **of its choosing**, and `Blocking` mode runs the
fetch synchronously on demand. The one constraint is `validate_domain`
(`mod.rs:739-765`), which rejects `://`, `/`, `:`, whitespace, and
leading or trailing dots. That rules out full URLs, ports, and IPv6 literals. But a
**bare IPv4 literal or internal hostname passes** (`169.254.169.254`,
`192.168.1.1`, `127.0.0.1`, `localhost`, `metadata.google.internal`).
The production `url_for_path` (`mod.rs:284`) builds `https://{domain}/`.

## SSRF reach
The service's `Http` client (`mod.rs:253-257`) is built via
`Http::builder()` with no `dns_resolver`, no redirect policy, and no
scheme restriction. It is the same wrapper type as the HTTP capability, with
no IP filtering on either.

- **(a) Page fetch: the gadget picks the host, giving a reachability oracle and a partial read.**
  `fetch_page_metadata` (`src-tauri/src/network/website_metadata/fetch.rs:80-88`) GETs
  `https://{domain}/`. Two signals flow back to the gadget via
  `LookupResult`. A **reachability oracle** (`Unreachable`, `ReachableNoData`, or `Hit`,
  `mod.rs:620-651`) lets a gadget port-scan or host-scan internal **HTTPS**
  services by iterating bare hosts, with no victim and no attacker page. There is also a **partial read**: an internal
  HTML endpoint's `<title>`, `<meta description>`, and OG tags are
  extracted (`src-tauri/src/network/website_metadata/metadata.rs:65-77`) and returned to the gadget in
  `CacheEntry.title` and `CacheEntry.description` (`host/website_metadata.rs:65-69`). That is a narrow internal-content read.
- **(b) Redirect amplification: an https-forced page fetch becomes arbitrary http SSRF.**
  No redirect policy is set, so reqwest's default applies
  (`Policy::limited(10)`, which follows `https` to `http` downgrades; validated
  in the HTTP capability todo against reqwest 0.13.2). A gadget passes `domain =
  attacker.example`. The attacker's `https://attacker.example/` returns
  `302 Location: http://169.254.169.254/latest/meta-data/...` (or
  `http://127.0.0.1:<port>/...`, or `http://10.x.y.z/...`), and the host
  follows it. `https://{domain}/` constrains only hop 0. Redirects
  reach arbitrary internal **http** targets and their reachability and
  HTML-metadata surfaces, per (a).
- **(c) Favicon fetch: a page-controlled URL and a blind GET.**
  `fetch_favicon_image` (`fetch.rs:167-240`) GETs `favicon_url`, which
  is resolved from `<link rel="icon" href="...">` via `base_url.join`
  (`metadata.rs:300`). An absolute `http://169.254.169.254/...` href is
  preserved. Since the gadget controls the page (its own domain, or via
  the redirect in (b)), it controls the favicon URL. That means any http or https
  host, port, or path, since reqwest rejects non-http(s) schemes. The only mitigations are a
  2 s timeout and a 256 KB cap. This path is **blind to the gadget**. The
  response is stored to disk (`FaviconStore`) and returned as an
  `EntryIcon::AssetIcon(url)` string, not as bytes, so the guest never
  reads the favicon body. At most it learns "image-shaped" (AssetIcon
  versus the `globe-alt` HeroIcon). This is a blind internal GET, with side effects and port
  probing, and not a read primitive.
- **(d) `data:` inline.** `decode_data_uri` (`metadata.rs:95-137`)
  accepts only `image/*`, decodes inline with no network access, and stores the
  bytes. A gadget can store arbitrary attacker bytes labelled as an
  image, including SVG. That is served via `torchsnap-favicon://` and rendered
  in an `<img>` (a non-scripted context), so embedded SVG script does not
  execute. The trust-chain risk here is low, and this is confirmed.

**Net:** gadget-driven SSRF with (i) a direct internal-HTTPS
reachability oracle from the `domain` argument, (ii) redirect-based
reach to arbitrary internal http endpoints, (iii) a narrow read of
internal HTML `<title>`, description, and OG metadata, and (iv) a blind favicon GET to
any internal http or https URL. It is a notch below the HTTP capability's
high, because the gadget cannot set method, headers, or body, and it cannot read arbitrary
response bodies (only extracted metadata fields and a reachable or
image-shaped bit; the favicon path is fully blind). It shares
the HTTP capability's exact root cause, and it is reached through a distinct,
install-consented capability. Severity: **medium (upper)**.

## Suggested fix

1. **Apply the resolve-then-pin egress guard from the HTTP capability todo, in the shared `Http` and `HttpBuilder`,** so that both `HttpCap` and website-metadata are covered by one implementation. This is the concrete resolution of that todo's open question about where the guard lives. The answer is the shared `Http`, because an `HttpCap`-only guard leaves this
   path as an independent unmitigated bypass, and duplicating the range table invites drift. The guard resolves the host and filters the returned
   `SocketAddr`s against the blocked-range table (loopback, link-local
   including `169.254.169.254`, RFC1918, ULA, CGNAT, and unspecified, for v4 and v6). It unwraps
   IPv4-mapped v6 and returns only vetted addresses. Because
   reqwest connects to exactly what the resolver returns, the checked
   IP is pinned to the connected IP. This **automatically covers every
   redirect hop**, since each hop reconnects through the resolver, which closes
   both the redirect and DNS-rebind vectors for website-metadata
   without a separate redirect policy. Website-metadata has no origin
   allowlist to keep honest across hops, unlike `HttpCap`, so the
   resolver guard is the primary and sufficient control here. Lowering
   the default hop limit is optional hardening.
2. **Default-deny private ranges for website-metadata, with no escape
   hatch.** Make the guard policy a per-instance `HttpBuilder`
   parameter. The HTTP capability's `allow_private_network` grant is not relevant here.
   The service fetches metadata for public registrable domains for
   search results, so there is no legitimate reason for it to reach
   internal or private IPs. It is also gadget-invokable, so the SSRF risk
   dominates any local-dev-preview nicety. Construct the
   website-metadata `Http` with private ranges blocked unconditionally.
   Only `HttpCap` instances whose gadget holds `allow_private_network`
   opt into relaxation. This is shared code, parameterized by policy, and the two
   callers pass different policies.
3. **Restrict fetch schemes to http and https explicitly** on both
   `fetch_page_metadata` and `fetch_favicon_image`. reqwest already
   rejects other schemes, but an explicit check makes the contract
   clear and rejects early. `data:` stays handled inline before the
   network call, image-mime-gated as today.
4. **Local-dev preview tradeoff (state as a decision):** the page
   fetch is gadget-driven, not user-typed, so the "developer previewing
   `localhost`" case is weak justification. Default-deny private
   ranges. If a local-preview affordance is ever wanted, gate it behind
   an explicit, separate opt-in rather than leaving the range open.

## Relations

- Relates to: [Redirects and DNS rebinding let HTTP gadget requests bypass the origin allowlist](01KWH4J9BPTRAYF451YZD2145M-redirects-and-dns-rebinding-let-http-gadget-requests-bypass-the-origin.md), the shared root cause and the egress-guard design; the guard placement above resolves that todo's open question
