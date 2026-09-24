---
kind: bug
severity: medium
status: open
area: [gadgets/zerotier/src/lib.rs]
tags: [unconfirmed]
---

# ZeroTier: auth/daemon state is frozen at enable() — daemon starting or dying later is misreported

## Problem

`runtime.auth_state` is computed exactly once, in `initialize()`
(`gadgets/zerotier/src/lib.rs:142-167`), from a single
`GET /status` probe. It is never revisited afterwards (only a
`manualToken` settings change re-runs `initialize`,
`lib.rs:122-132`). Two stale-state directions follow:

1. **Daemon starts after Torchsnap.** `enable()` ran while
   `zerotier-one` was down → `auth_state =
   DaemonUnreachable`. Every subsequent ZT query shows the
   "ZeroTier daemon not running" failure entry
   (`failure_entry`, `lib.rs:313-349`) — permanently, even
   after the daemon is up and reachable, until the gadget is
   disabled/re-enabled or the token setting is touched. Since
   `failure_entry` returns before any live fetch, the gadget
   never even attempts a request that would prove the daemon is
   back. For a daemon commonly started on demand, this is the
   normal sequence, not an edge case.

2. **Daemon dies after enable().** `auth_state` stays
   `Validated`, so no failure entry is shown; `current_live_state`
   gets `Err` from `list_networks` and returns
   `unwrap_or_default()` — empty live state (`lib.rs:462-488`).
   All joined networks silently render as "Stored" (KnownOnly)
   with a "Connect" action; activating one issues a join against
   a dead daemon and surfaces a raw error string.

Related comment drift: the `Runtime.network_cache` field doc says
"The slot stores the full `Result` so cache hits don't lose error
context" (`lib.rs:70-73`), but the only consumer immediately
discards the error (`result.unwrap_or_default()`, `lib.rs:487`) —
the preserved context is never used.

## Suggested fix

Derive reachability from the *live fetch outcome* instead of the
enable-time snapshot:

- In `current_live_state`, keep the `Result` and let
  `build_search_entries` distinguish "fresh error" (show the
  daemon-unreachable failure entry, or a stale-data badge) from
  "fresh data" (clear a previously-set `DaemonUnreachable`).
- For direction 1, let a failed enable-time validation fall
  through to normal search flow: attempt the (1 s rate-limited)
  live fetch and upgrade `auth_state` to `Validated` when it
  succeeds.

The 1-second `RateLimitCache` already bounds probe frequency, so
this adds no daemon load.
