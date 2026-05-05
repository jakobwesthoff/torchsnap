# 41. ZeroTier plugin architecture

Date: 2026-04-30

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

`zerotier-one` runs as a local daemon that exposes its full management
surface as a small HTTP API on `http://localhost:9993`. Users today
manage networks through the official menu-bar UI on macOS or the CLI
on every other platform. Both flows pull the user out of their
current keyboard context to perform what is, ergonomically, a
launcher-shaped action: "given a known network, connect / disconnect;
given a new ID, join."

A Torchsnap plugin is the natural fit. The launcher is already where
users go for keyboard-driven control of their machine; ZeroTier
membership is exactly the kind of "name → action" surface the
launcher does well. The plugin needs to surface every joined network
plus everything the user has previously joined-then-left, with the
right action attached, and degrade gracefully when the daemon is
absent or the user's auth token is misconfigured.

The plugin also drove four host-API additions that benefit every
future plugin: a read-only filesystem capability with a manifest-
declared allowlist, richer transport-error categorization, a
per-request insecure-TLS flag, and an `OpenSettings` action variant
the host routes to the originating plugin's settings panel. Each is
recorded in its own ADR-level discussion (see "Related ADRs"
below); this one captures the integration shape that exercises them.

## Decision

### Plugin shape — query-driven, no custom view

The plugin emits standard `ScoredEntry` results from `search()`. No
launcher view or catalog entries: typing the launcher hotkey and a
network name (or its 16-hex-char ID) is the entire interaction. The
settings panel hosts the manual-token field and the
remembered-networks management surface; ad-hoc state inspection
reuses the launcher's standard result-render path.

### Three-tier state model

For each network ID the plugin tracks one of three states, derived
from the union of live `GET /network` and the plugin's own history
table:

\| State | Origin | Meaning |
\|---|---|---|
\| `Connected` | live, `status == OK` | traffic flowing |
\| `JoinedOffline` | live, any other status | joined but not connected (`REQUESTING_CONFIGURATION`, `ACCESS_DENIED`, `AUTHENTICATION_REQUIRED`, `NOT_FOUND`, `PORT_ERROR`, `Unknown`) |
\| `KnownOnly` | history only | network we have seen before, daemon has left it |

The state determines the entry's primary action label
(`Disconnect` for the first two, `Connect` for the last) and the
subtitle badge. `Forget` is always available as a secondary action.

### Cache and rate-limiting

`search()` is host-blocking — every keystroke holds the launcher UI
thread until the plugin returns. A naïve per-keystroke
`GET /network` would expose every plugin user to the request
timeout's worst case (a hung daemon freezes the launcher for the
full timeout window). Two coordinated mechanisms:

* A **200 ms timeout** on `search()`-time fetches via the
  `timeout-ms` field on `http-request`. Localhost daemons normally
  answer in single-digit milliseconds; the timeout caps the worst
  case at a perceptible-but-acceptable 200 ms.
* A **1 s rate-limited cache**. Successive keystrokes within the
  TTL share the prior fetch's result; the daemon sees one fetch per
  second per launcher session. Mutating actions
  (Connect / Disconnect / Forget) call
  `RateLimitCache::invalidate` to make the next render reflect
  post-action state without waiting for TTL.

This is rate-limiting, not debounce: the timer resets from the
moment of the *fetch*, not from the most recent access.

### Auth-token resolver

ZeroTier writes the daemon's auth token at install time and never
rotates it. The resolver therefore runs once on `enable()` and is
re-invoked only when the user edits the manual-paste setting:

1. Per-OS canonical paths in priority order. macOS tries the
   system path then the user-side 0644 copy created by the
   official UI; Linux tries the daemon path; Windows the
   `ProgramData` path. Reads go through the new `fs::read_file`
   host import — the manifest's `[permissions.fs]` block declares
   each candidate.
1. Manual-paste fallback from the `manualToken` plugin setting.
1. If neither produces a token, the plugin marks itself as
   `Unconfigured`. `search()` queries with ZT context (matched
   network or bare ID) surface a "ZeroTier token not configured"
   entry with `ActionId::OpenSettings` so the user can fix the
   problem without leaving the launcher.

A single `GET /status` request validates the token at resolve
time. The result drives the
`Validated`/`Rejected`/`DaemonUnreachable`/`Unconfigured`
classification used by both the failure-state launcher entries and
the settings panel's diagnostic text.

### `saved_networks.json` merge

The official macOS UI persists previously-joined networks in
`~/Library/Application Support/ZeroTier/saved_networks.json`. The
file is the UI's private cache, but the plugin can read it
(again via `fs::read_file`) and merge entries into its history
table on plugin instantiation, plus on demand via the settings
panel's "Re-import" button.

`INSERT OR IGNORE` semantics: imported entries fill in only when
the plugin doesn't already know about them. Daemon observations
remain authoritative once we have any, and the plugin never
writes back to the JSON file.

### Score tiers

Plugin entries carry these base scores, summed with the per-match
nucleo score:

\| Class | Score |
\|---|---|
\| Connected | 750 + nucleo |
\| Joined offline | 450 + nucleo |
\| Known only | 250 + nucleo |
\| Synthetic Connect-by-ID | 125 |

These slot between the existing host anchors (bangs at 1000,
open-url at 500) so connected ZeroTier results rank between bangs
and URLs, joined-offline near URLs, and stored networks below.
Frecency layers up to ~3000 of additional score on top, so
frequently-used networks rise to the top regardless of base
tier. The `nucleo-matcher` crate is pinned to `=0.3.1` to match
the host catalog matcher exactly.

### Settings panel

Built from the SDK's `<Section>`, `<Entry>`, and the new shared
`<List>` component. Two areas:

* Token field, disabled with an "Auto-detected" notice when the
  resolver successfully read a per-OS path; enabled with
  per-state diagnostic text otherwise.
* Remembered-networks list with per-row Forget, plus toolbar
  Clear-all (with confirmation) and macOS-only Re-import.

### Out of scope for v1

* Unix domain socket transport — separate todo
  (`01kqewdadvnfgy90672x3e3fq6-fetch-unix-socket-transport.md`).
* Streaming responses on fetch (deferred per ADR 0038).
* File watching on `saved_networks.json`. Merge-on-instantiation
  plus a manual Re-import button cover the cases that matter; a
  dedicated `fs-watch` interface can wait for a real consumer.
* Add-by-ID-without-joining in settings. Connect-via-launcher is
  the canonical entry point.
* Force-reconnect action. User can Disconnect then Connect.
* Prefix routing (`zt:` / `zerotier:` / `join:`). Trivial to add
  later if useful; not necessary for v1 since the bare-16-hex-char
  trigger handles the join-by-id case without ambiguity.

## Consequences

### Positive

* The plugin exercises every host-API addition the work landed
  end-to-end (`fs`, the richer fetch error model, `OpenSettings`,
  the shared `<List>` SDK component). Real-consumer pressure
  validated each before any other plugin adopts them.
* Failure-state entries make the plugin self-diagnosing: a user
  whose daemon is down or whose token doesn't work sees a
  one-keystroke path to fix it from inside the launcher, with no
  separate troubleshooting flow.
* Cache invalidation on mutating actions keeps the user's mental
  model in sync with reality without timer-driven polling.

### Negative

* The 200 ms `search()`-time timeout is tight enough to surface as
  a "no results" flicker if the daemon is genuinely slow (rather
  than hung). Acceptable trade for not freezing the launcher; can
  be tuned upward if real users hit the regression.
* Settings frontend and Rust handler share a JSON RPC contract
  with no schema enforcement. A typo in a method name or response
  shape on either side fails at runtime, not at build time.
  Acceptable for now — the surface is small (six methods) — but a
  shared types crate is the right v2 step.

### Verification

End-to-end verification happens via real-daemon smoke tests on
each supported platform:

1. Daemon running, joined to one network → typing the network's
   name surfaces a Connected entry with the correct subtitle
   (`● Connected · 10.x.x.x`).
1. Typing 16 hex characters not in history → "Connect to network
   `<id>`" entry; activating it joins and inserts a history row.
1. Triggering Disconnect on a connected entry → next query within
   1 s sees the Stored badge.
1. Triggering Forget on any entry → the row disappears from the
   settings panel's list.
1. Stopping the daemon → next ZT-context query shows
   "ZeroTier daemon not running"; entry is non-actionable other
   than Dismiss.
1. Settings panel: token field disabled with auto-detected text on
   a healthy macOS install; enabled with explanatory info on a
   Linux install where the daemon path is not group-readable.

The plugin's pure logic (api types parsing, intent detection,
fuzzy matching, score tiering, history JSON parsing, cache
semantics) is covered by 39 unit tests inside the plugin crate.
The host-import-dependent paths (auth resolver's `fs::read_file`
calls, api client's `http::fetch` round-trips, history's
SQLite operations) are exercised by the smoke-test verification
above and by the implicit cross-plugin compatibility gate (every
existing plugin recompiles cleanly against the SDK bump).

## Related ADRs

* ADR 0035 — plugin distribution via bundled and user-installable archives
* ADR 0036 — plugin trust model and deferred signing
* ADR 0038 — WASM plugin HTTP API (extended in this work with
  richer error variants and `insecure-tls`)

## Related todos

* `01kqaqzdma111a817snbna5n8b-zerotier-one-integration.md` —
  iterative design discussion that produced this ADR.
* `01kqewdadvnfgy90672x3e3fq6-fetch-unix-socket-transport.md` —
  deferred Unix socket transport, needed for Docker / podman / etc.
  plugins but not for ZeroTier.
* `01kqf1at1he1em7kfw6xa2rwfd-migrate-calculator-history-to-list-component.md`
  — exploratory follow-up on whether the calculator plugin's
  history view should migrate to the shared `<List>` component.
* `01kqf1xsn6rb650f5a793wa5s4-refactor-manifest-rs.md` —
  unrelated cleanup surfaced during this work.