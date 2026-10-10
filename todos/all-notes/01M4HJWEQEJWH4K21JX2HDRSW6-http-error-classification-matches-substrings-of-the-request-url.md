---
title: "HTTP error classification matches substrings of the request URL"
kind: bug
component: gadget-host
impact: low
tags: [error-handling, capabilities]
---
# HTTP error classification matches substrings of the request URL

The HTTP capability picks the `HttpError` variant a gadget receives by substring-matching the error text, which embeds the request URL, so a URL can steer the classification. Found in a review of the HTTP capability (`HttpCap`); the review's date is not recorded in the original todo.

## Problem
`from_request_error` (`src-tauri/src/caps/http.rs:73-111`) does the right thing
first (a typed `re.is_timeout()`), then falls back to lowercasing the
entire error chain, which embeds the request URL, and substring-
matching `"tls"`, `"dns"`, `"ssl"`, `"handshake"`, `"certificate"`,
and similar. A URL like `https://tls.example.com/` can force an unrelated
failure into `TlsFailed` or `DnsFailed`.

## Impact
Low and cosmetic. It only selects
which `HttpError` variant the guest receives, affects no allow or deny
decision or control flow, and discloses nothing the guest didn't
supply.

## Suggested fix
Replace substring matching over `Display` with
reqwest typed predicates. `is_timeout` is already used, and `is_connect`,
`is_redirect`, `is_body` or `is_decode`, `is_request` or `is_builder`, and `is_status` are available. Caveat: reqwest 0.13 exposes
no `is_dns()` or `is_tls()`, so `DnsFailed`, `ConnectionRefused`, and `TlsFailed`
cannot be reconstructed cleanly from predicates alone. If
that granularity is needed, walk `std::error::Error::source()` and
downcast to concrete types (`io::Error` kind, rustls error types)
rather than the URL-contaminated `Display` string. Otherwise, fold
the DNS and TLS ambiguity into a generic transport error. Stop matching over
strings that embed the URL. Once the SSRF fix is in place, a hop rejected by the
redirect policy or the resolver surfaces as `is_redirect` or a connect
error. Map those to `PermissionDenied` (or a new redirect-blocked
variant) so the guest gets an accurate signal.

## Relations

- Relates to: [Redirects and DNS rebinding let HTTP gadget requests bypass the origin allowlist](01KWH4J9BPTRAYF451YZD2145M-redirects-and-dns-rebinding-let-http-gadget-requests-bypass-the-origin.md), the SSRF fix adds the redirect and resolver rejections that need a mapped error
