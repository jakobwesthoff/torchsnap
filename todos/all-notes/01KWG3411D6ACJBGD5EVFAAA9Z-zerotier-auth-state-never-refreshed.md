---
kind: bug
severity: medium
status: open
area: [gadgets/zerotier/src/lib.rs]
tags: [unconfirmed]
---

# ZeroTier: daemon dying after enable() is misreported as empty state

## Problem

The "daemon starts after Torchsnap" direction of this bug is fixed:
`should_retry_auth`/`auth_retry_due` (`gadgets/zerotier/src/lib.rs:105-126`)
re-run `initialize()` from `search()` on a 5-second throttle while
`auth_state` is not `Validated`, so a daemon that comes up after
`enable()` is picked up without a restart.

The other direction remains. Once `auth_state` reaches `Validated`,
`auth_retry_due` never re-checks it (`lib.rs:106-108`: `if state ==
AuthState::Validated { return false; }`). If the daemon dies after
that point, `current_live_state` (`lib.rs:529-555`) gets `Err` from
`list_networks` and returns `unwrap_or_default()` (`lib.rs:554`):
empty live state, no error surfaced. `failure_entry`
(`lib.rs:393-406`) only inspects `runtime.auth_state`, which is
still `Validated`, so it returns `None` and no warning entry is
shown. All joined networks silently render as "Stored" (KnownOnly)
with a "Connect" action; activating one issues a join against a dead
daemon and surfaces a raw error string.

Related comment drift: the `Runtime.network_cache` field doc still
says "The slot stores the full `Result` so cache hits don't lose
error context" (`lib.rs:76-79`), but the only consumer immediately
discards the error (`result.unwrap_or_default()`, `lib.rs:554`), so
the preserved context is never used.

## Suggested fix

Derive reachability from the *live fetch outcome*, not only from the
enable-time/retry-time `auth_state`:

- In `current_live_state`, keep the `Result` and let
  `build_search_entries` (`lib.rs:349-384`) distinguish "fresh
  error" (show a daemon-unreachable failure entry, or a stale-data
  badge) from "fresh data" (keep `auth_state` at `Validated`).
- On a fresh error, downgrade `runtime.auth_state` to
  `DaemonUnreachable` so `auth_retry_due` starts retrying again and
  `failure_entry` fires on the next query.

The 1-second `RateLimitCache` already bounds probe frequency, so
this adds no daemon load.
