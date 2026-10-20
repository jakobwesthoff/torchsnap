---
title: "HTTP gadget requests read response bodies with no size limit by default"
kind: bug
component: gadget-host
tags: [security, memory, capabilities]
---
# HTTP gadget requests read response bodies with no size limit by default

When a gadget omits `max_body_size`, the HTTP capability buffers the whole response in host memory, so a gadget can crash the host process with an oversized response. Found in a review of the HTTP capability (`HttpCap`); the review's date is not recorded in the original todo.

## Problem
`DEFAULT_MAX_SIZE = u64::MAX` (`src-tauri/src/network/http.rs`). `send` resolves
`max_size = self.max_size.unwrap_or(self.default_max_size)` (in `send`),
and `HttpCap::fetch` forwards the guest's `max_body_size` only when
it is `Some` (`HttpCap::fetch` in `src-tauri/src/caps/http.rs`). When the guest omits it, the effective
limit is `u64::MAX`. The Content-Length early reject (in `send` in `network/http.rs`)
and the streaming total check (in `send`) never fire, and `bytes()`
buffers the full response into host memory. This contradicts the WIT
contract, which says `max-body-size: none` "delegates to the host
default (guards against unbounded downloads)" (`wit:273-274`). The host
default is `u64::MAX`, which guards nothing.

## Impact
Medium. It is an OOM DoS with a one-line trigger: request a large or chunked-forever resource
and omit `max_body_size`. It crashes the whole host process and every other
gadget with it. There is a secondary amplification: `bytes()` holds the full body
in memory, so N concurrent requests cost N times the cap, even with a per-request
cap.

## Suggested fix
Honour the WIT promise. When the guest omits
`max_body_size`, apply a real host default instead of `u64::MAX`, and
clamp any guest-supplied value to a hard ceiling so a gadget cannot
re-request `u64::MAX`. Recommended: 32 MiB when omitted, with a hard ceiling
of about 256 MiB. Keep the general `Http` wrapper's `u64::MAX` default only
for trusted host-internal callers (website metadata), if desired. The
gadget-facing path must always be bounded. The residual concurrency
amplification (N times the cap) is a follow-on note, not blocking.

## Considered but not folded in
A per-gadget concurrent-request or total-in-flight-bytes budget to
close the N-times-cap memory amplification. This is a follow-on to this fix.
