---
kind: feature
status: in-progress
---

# ZeroTier One Integration

Design in progress. Shipped as a dedicated WASM gadget
behind one prerequisite (file-access WIT layer). Fetch WIT is already
sufficient.

## Goal

Integrate Torchsnap with the local `zerotier-one` service so the user can,
from within Torchsnap:

- See all ZeroTier networks the machine is currently joined to, with full
  per-network detail (managed addresses, managed routes, allow-flags,
  device, MAC, status, type) — equivalent to the per-network submenu of
  the official macOS ZeroTier UI.
- See networks the user has previously joined but is not currently joined
  to (the unchecked rows in the official UI's main menu).
- Connect to a previously-known network, leave a currently-joined
  network, and join a brand-new network — the latter being added to
  Torchsnap's own history.

## Implementation kickoff

When this work begins, **start by copying `gadgets/template/` into a
new `gadgets/zerotier/` directory** and renaming the crate / gadget
metadata accordingly. The template carries the canonical layout
(Cargo manifest, `manifest.toml`, `define_gadget!` registration,
SDK prelude wiring) and is the supported starting point for new
gadgets. Do not hand-roll gadget scaffolding from the SDK directly.

After copying:

1. Rename the crate (`Cargo.toml`), the gadget id and metadata
   (`manifest.toml`), and the registered struct.
2. Add the gadget entry to `gadgets/bundled.toml` once the
   implementation is mature enough for release bundles (per
   CLAUDE.md's "Gadget build pipeline" section). During development
   the debug loader picks it up automatically.
3. Declare the runtime permissions described in this todo —
   `[permissions.http]` with `origins = ["http://localhost:9993"]`,
   and `[permissions.fs]` with the auth-token / saved-networks
   path patterns once the fs WIT prerequisite lands.

## Architecture decisions (settled)

- **Dedicated WASM gadget**, not a host integration. Matches current
  architectural direction; feature is self-contained.
- **History storage:** SQLite via the existing gadget SQL host import.
  Gadget-home pattern from CLAUDE.md / ADR 0035 already provides the
  per-gadget sqlite location.
- **`saved_networks.json` policy:** merge-once-per-session. On gadget
  instantiation (start of a launcher session), read the file (macOS
  only) and upsert any unknown entries into the gadget's own
  history. Idempotent; no watcher needed; naturally picks up entries
  created by the official UI between sessions. A manual re-import
  button in gadget settings handles the mid-session case.
  Torchsnap never writes back to that file.
- **Gadget shape:** query-driven launcher gadget (not a panel app).
  See "Launcher behavior model" below for query intents, entry
  display, and cache strategy. Gadget settings (a separate UI
  surface) hosts the manual-token config field and the
  remembered-networks management view.
- **Auth token UX:** auto-detect across all known per-OS paths on
  gadget instantiation. If any path yields a readable token,
  validate it once via `GET /status` and use it; disable the manual
  config field with an "auto-detected" notice. If none work,
  surface a manual-paste config field with an explanatory info box
  describing where each OS stores the token and how to obtain it.
  The pasted token is stored in gadget storage and used as the
  override. Token is not re-resolved during a session unless the
  user edits the config field.
- **Multi-platform from v1.** macOS, Linux, Windows. Each is just a
  different list of candidate paths feeding the same resolver, with
  the same manual-paste fallback when none are readable.
- **HTTP client approach:** hand-rolled `serde` types over the existing
  fetch host import. Surface area is ~5 endpoints; a generator would
  add more weight than it saves.

## Local service API summary

`zerotier-one` exposes a small HTTP API on `http://localhost:9993`. Auth
is a single header `X-ZT1-Auth: <token>`. Spec:
<https://docs.zerotier.com/openapi/service/v1.json> (also archived at
`/Users/jakob/Downloads/api-1.yaml` during this design discussion).

Endpoints relevant to Torchsnap:

| Purpose | Call |
|---|---|
| List joined networks (full detail) | `GET /network` |
| Single network detail refresh | `GET /network/{id}` |
| Join / update membership | `POST /network/{id}` (body: Network JSON, often empty / minimal allow-flags) |
| Leave a network | `DELETE /network/{id}` |
| Node-level status, online flag, address | `GET /status` |

Out of scope: `/controller/*` (only relevant for nodes hosting networks)
and `/peer*` (per-peer link state, not needed for the network panel).

### Fields needed for the per-network detail view

All come from a single element of `GET /network`:

| UI label | JSON field |
|---|---|
| Network ID | `id` |
| Name (nice) | `name` |
| Allow Managed Addresses | `allowManaged` |
| Allow Assignment of Global IPs | `allowGlobal` |
| Allow Default Route Override | `allowDefault` |
| Allow DNS Configuration | `allowDNS` |
| Ethernet (MAC) | `mac` |
| Device | `portDeviceName` |
| Type | `type` (e.g. `PRIVATE`, `PUBLIC`) |
| Status | `status` (`OK`, `REQUESTING_CONFIGURATION`, `ACCESS_DENIED`, `NOT_FOUND`, `AUTHENTICATION_REQUIRED`, `PORT_ERROR`) |
| Managed Addresses | `assignedAddresses[]` |
| Managed Routes | `routes[]` (`target`, `via`, `flags`, `metric`) |
| DNS | `dns.{domain, servers}` |
| MTU | `mtu` |

"Currently connected" is derived from `status == "OK"`, not merely from
presence in the list — entries with status `REQUESTING_CONFIGURATION` or
`ACCESS_DENIED` are joined but not connected.

## Auth token resolution

Per-OS canonical paths (resolver tries each in order, first readable wins):

- **macOS**
  1. `~/Library/Application Support/ZeroTier/One/authtoken.secret`
     (root-owned but `0644`; world-readable; created by the official UI
     on first run)
  2. `/Library/Application Support/ZeroTier/One/authtoken.secret`
     (`0600 root:wheel`, normally unreadable to user processes)
- **Linux**
  1. `/var/lib/zerotier-one/authtoken.secret` (`0600 root:root`;
     readable only if the user is in a `zerotier-one` group on
     distros that ship one)
- **Windows**
  1. `C:\ProgramData\ZeroTier\One\authtoken.secret` (admin-restricted)

Fallback chain on every gadget start:

1. Try every per-OS canonical path; first readable wins.
2. If none readable, fall back to the user-supplied token from gadget
   config. If that exists, use it. UI: config field disabled with
   "auto-detected" notice when (1) succeeded; otherwise enabled with
   an explanatory info box describing where to find the token.
3. If neither produces a token, surface a clear error UX in the
   gadget's main panel.

## History storage

The local daemon does **not** track networks the user has previously
joined and left. Once `DELETE /network/{id}` is issued, `networks.d/`
loses the `<id>.conf` and the daemon has no further memory of it.
`GET /network` therefore only enumerates **currently joined**
memberships.

The official macOS UI's "previously joined" list is **not** an API
feature. It is maintained client-side by the UI itself in:

```
~/Library/Application Support/ZeroTier/saved_networks.json
```

Format: flat JSON object keyed by network ID. Each value:

```json
{
  "id": "<networkid>",
  "name": "<nice name>",
  "settings": "<stringified Network JSON, exactly the GET /network/{id} response>"
}
```

Ownership: written by the user-mode macOS UI (lives under user
`~/Library`, daemon runs as root and never writes here). **No
equivalent file exists on Linux or Windows** — it is a macOS-UI-only
artifact.

### Torchsnap-owned history (SQLite)

Schema sketch (refine during implementation):

```sql
CREATE TABLE networks (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    first_seen    INTEGER NOT NULL,  -- unix ms
    last_seen     INTEGER NOT NULL,
    last_status   TEXT,              -- last observed status enum
    last_snapshot TEXT                -- last full Network JSON (nullable)
);
```

Backed by the per-gadget sqlite file in
`<app_data_dir>/gadget-home/<gadget-id>/state.sqlite3`.

### `saved_networks.json` import

Mode: **merge once per gadget instantiation** (macOS only). When a
new launcher session starts and the gadget is first instantiated:

1. If the file exists and is readable, parse it.
2. For each entry, `INSERT OR IGNORE` a row keyed by network ID.
   Existing rows are not overwritten — Torchsnap's own observations
   are authoritative once we've seen a network live.
3. No write-back to `saved_networks.json` ever.

Linux / Windows skip this step entirely. The settings panel exposes
a manual "Re-import from ZeroTier UI" button (see Settings UI
section) for users who add networks via the official UI and want to
pick them up without ending the launcher session.

## Launcher behavior model

The gadget is query-driven. Each user keystroke triggers a query
invocation; the gadget returns zero or more entries that the launcher
ranks alongside other gadgets' results. State (history table, daemon
response cache, resolved auth token) persists across query
invocations within a single launcher session.

### Internal state per network ID

For each known network, the gadget tracks one of three states (union
of live daemon response and history table):

- **connected** — in live `GET /network` with `status == "OK"`.
- **joined-but-offline** — in live `GET /network` with any other status.
- **known-but-not-joined** — in history only, not in live.

### Query intents

Two intents fire from a query, possibly together. Both rely on
substring / prefix matching against name and ID — no special keyword
required. Intent B fires on input shape alone.

**Intent A — match existing networks.** Fuzzy-match the query against
the merged set (live + history) by network name and by ID prefix.
Each match returns one entry with a state badge and action set.

**Intent B — surface a "Connect" action for an unknown ID.** Fires
when the input is exactly 16 hexadecimal characters (case-insensitive,
separators stripped). If the resulting ID already exists in
history, Intent A's entry takes precedence — no duplicate "Connect"
entry. Otherwise produce a synthetic entry "Connect to network
`<id>`" whose action is `POST /network/{id}` followed by a history
insert.

Optional convenience: a leading `zt` / `zerotier` / `join` keyword
followed by a partial ID also fires Intent B for ID prefixes shorter
than 16 chars. Stretch goal — confirm useful in practice before
implementing.

### Entry display per state

| State | Badge | Primary action | Secondary action (Forget) |
|---|---|---|---|
| Connected (`OK`) | "● Connected" + assigned IPs | **Disconnect** (`DELETE`) | **Forget** — `DELETE` then drop history row |
| Joined, requesting config | "○ Connecting…" | **Disconnect** | **Forget** |
| Joined, access denied | "⚠ Access denied" | **Disconnect** | **Forget** |
| Joined, auth required | "⚠ Auth required" | **Disconnect** | **Forget** |
| Joined, port error | "⚠ Port error" | **Disconnect** | **Forget** |
| Joined, not found by daemon | "⚠ Network not found" | **Disconnect** | **Forget** |
| Known, not joined | "Stored" + last-seen | **Connect** (`POST`) | **Forget** (drop history row) |
| Synthetic Intent B (unknown ID) | "Connect to network `<id>`" | **Connect** + history insert | — |

Vocabulary follows the official ZeroTier UI: **Connect / Disconnect**
at the user-facing layer, even though the wire calls are POST/DELETE
("join / leave"). "Forget" is always history-side; on currently
joined networks it implies Disconnect first.

### Wire-call recap

- **Connect** (any state): `POST /network/{id}`, empty body.
  Daemon makes no distinction between "new" and "known" — the only
  difference is whether the gadget already has a history row.
- **Disconnect**: `DELETE /network/{id}`. History row stays
  (downgrades to "known, not joined").
- **Forget**: drop the history row. If the network is currently
  joined, do `DELETE /network/{id}` first.

### Failure-state entries

When the daemon or auth isn't reachable, the gadget returns a single
synthetic entry under Intent A so the user sees something actionable.
These only fire when at least one ZT-shaped match would otherwise
have been produced (i.e. the user typed something that resolves to
ZT context); they don't pollute unrelated queries.

| Condition | Entry | Action |
|---|---|---|
| Daemon unreachable (connection-refused on `/status`) | "⚠ ZeroTier daemon not running" | (none) |
| No auth token resolved | "⚠ ZeroTier token not configured" | open gadget settings |
| Token rejected (401 on validation) | "⚠ ZeroTier authentication failed" | open gadget settings |

## Cache and rate-limiting

The gadget caches the last `GET /network` (and `GET /status`)
response and rate-limits refresh:

- TTL: **1 second** (const, easily bumped). Implemented as
  rate-limiting, **not** debounce: cached values are returned for
  up to TTL since the *last fetch*; the timer does not reset on each
  keystroke. The launcher stays snappy and the daemon is hit at
  most once per second per session.
- Cache constructor takes the TTL parameter so tests / tuning can
  override.
- Mutating actions (Connect / Disconnect / Forget) **invalidate the
  cache immediately** so the next query reflects post-action state
  without waiting for TTL.
- No timer-driven background refresh. Refresh happens only as a
  side effect of a query that finds an expired cache.

## Auth resolver lifecycle

- **On gadget instantiation (start of launcher session):**
  1. Walk the per-OS candidate paths and the manual-paste config
     value; pick the first readable token.
  2. Validate with a single `GET /status` request.
  3. Cache the resolved token for the rest of the session.
- **Runtime invalidation triggers:**
  - User edits the manual-paste field in settings → resolver runs
    again.
  - (Not needed: ZeroTier writes the token once at install and
    does not rotate it. We do not poll for changes.)
- A 401 mid-session is treated as a configuration error
  (token-rejected failure-state entry above), not a re-resolve
  trigger — the token didn't change, the daemon's view of it did.

## Prerequisite status

### Fetch WIT layer — VALIDATED for ZeroTier, but enhancements proposed

Investigated against the existing implementation:

- **WIT:** `gadgets/gadget-sdk/wit/torchsnap-gadget.wit` lines 241-303
  (`interface http`).
- **Host:** `src-tauri/src/wasm/runtime/host/http.rs` (entry point,
  origin check, method translation) and `src-tauri/src/network/http.rs`
  (reqwest wrapper).
- **ADR:** `docs/adr/0038-wasm-gadget-http-api.md` (accepted 2026-04-19).

Confirmed sufficient for ZeroTier:

- Arbitrary request headers — `headers: list<tuple<string, string>>` is
  passed verbatim to reqwest at `host/http.rs:159-161`. No allowlist.
  `X-ZT1-Auth` and `Content-Type: application/json` work without change.
- Request body — `body: option<list<u8>>`. POST with body, empty-body
  POST (`body: none`), and DELETE without body all supported
  (`host/http.rs:162-164`).
- Response body — full `list<u8>` returned to guest; gadget
  serde-parses JSON itself.
- Plain HTTP loopback — **no scheme allowlist, no loopback rejection**.
  `check_http_origin` (`host/http.rs:104`) is the only gate; it
  string-compares the URL's serialized origin against the manifest
  allowlist. `http://localhost:9993` produces origin
  `http://localhost:9993` which passes when declared.
- Methods — GET, POST, DELETE all native variants
  (`torchsnap-gadget.wit:259-267`).

**Action required for ZeroTier specifically:** the gadget's
`manifest.toml` must declare:
```toml
[permissions.http]
origins = ["http://localhost:9993"]
```

#### Fetch WIT enhancements — implementable separately, ahead of ZeroTier

These are not strictly required by ZeroTier, but the work is cheap,
benefits every future gadget, and is easier to fold in while we're
already opening the WIT for the fs layer. They form a self-contained
prerequisite that can ship before the ZeroTier gadget without
blocking it.

**1. Per-request timeout.** Add to `http-request`:
```wit
record http-request {
    // ... existing fields ...
    timeout-ms: option<u32>,   // None = host default
}
```

Host applies a default of **10 seconds** when `timeout-ms` is `none`.
Gadget can override per-request. Host caps the upper bound (e.g. 5
minutes) so a misbehaving gadget can't disable timeouts entirely.

**2. Richer connection-class error variants.** Replace the current
`http-error` with a transport-level error variant. Critical
invariant: **any HTTP response with status 100–599 is delivered as
`Ok(http-response)`**, including 4xx/5xx — those are not errors at
the transport layer. `http-error` covers only the cases where no
HTTP response exists.

```wit
variant http-error {
    connection-refused,
    timeout,
    dns-failed(string),
    tls-failed(string),
    invalid-url,
    permission-denied,
    other(string),
}
```

`http-response.status` (already present) carries the HTTP status code
on every successful response, so gadgets distinguish 200 / 401 / 404
/ 5xx in their domain logic without string-matching.

**3. Per-request insecure-TLS flag.** Some local daemons expose APIs
over self-signed HTTPS (e.g. Docker over TLS, k3s API). Per-request
flag only — no manifest gate:

```wit
record http-request {
    // ...
    insecure-tls: bool,   // default false
}
```

Rationale: the origin allowlist already gates *which* endpoint the
gadget reaches. Whether to verify the cert when talking to that
specific endpoint is a transport detail, not a separate capability.
A hostile gadget could declare any manifest flag anyway, and the
user already trusts the gadget enough to grant the origin.

Host implementation: a parallel `reqwest::Client` built with
`danger_accept_invalid_certs(true)` is selected when the per-request
flag is `true`. Not relevant to ZeroTier (plain HTTP loopback), but
trivial to add once and avoids future WIT churn.

**4. Streaming responses.** **Deferred.** ADR 0038 already defers
this. Real cost: streaming WIT type, backpressure, gadget-side
cancellation — not justified by ZeroTier or any near-term gadget
idea. Re-open when the first gadget requiring SSE / chunked event
streams is concretely scoped.

**5. Unix domain socket transport.** **Deferred to its own todo
(`01kqewdadvnfgy90672x3e3fq6-fetch-unix-socket-transport.md`).** Not
required by ZeroTier; not architecturally locked-in by deferring.
Re-open when a gadget needing it (Docker, podman, systemd, pueue,
…) is concretely scoped.

### File-access WIT layer — DOES NOT EXIST, blocking prerequisite

A new host import is required. Proposed shape (open for refinement):

#### Capability model

- **Read-only in v1.** ZeroTier needs no writes; defer the write
  permission discussion until a gadget actually requires it.
- **Manifest-declared path allowlist with glob support**, mirroring
  the existing `[permissions.http]` shape:

  ```toml
  [permissions.fs]
  read = [
      "{user-config}/ZeroTier/One/authtoken.secret",
      "{system-config}/ZeroTier/One/authtoken.secret",
      "/var/lib/zerotier-one/authtoken.secret",
      "{user-config}/ZeroTier/saved_networks.json",
  ]
  ```

  Patterns may use `*` (matches a single path segment) and `**`
  (matches across segments). `?`, character classes, and brace
  alternation are **not** part of the v1 syntax — keep the surface
  small and unambiguous.

- **Placeholder set (minimal v1):**
  - `{user-config}` — `~/Library/Application Support` (macOS),
    `$XDG_CONFIG_HOME` or `~/.config` (Linux), `%APPDATA%` (Windows).
  - `{system-config}` — `/Library/Application Support` (macOS),
    `/etc` (Linux), `%PROGRAMDATA%` (Windows).
  - `{user-home}` — included only if a real use case appears.
- **Gadget-home is out of scope for this layer.** No `gadget-home`
  accessor in the fs interface. Gadgets that need persistent
  per-gadget storage use the existing SQL host import (this gadget
  does). If a future gadget actually needs filesystem-shaped
  per-gadget storage, design a separate `gadget-storage` interface
  then.

#### Glob safety — implementation strategy

The matcher itself is not the security boundary; **path
normalization order** is. Proposed pipeline:

**Library:** [`globset`](https://docs.rs/globset) — battle-tested
(ripgrep, cargo, watchexec), compiles a pattern set to an automaton,
configurable so `*` does not cross `/`. Build with
`GlobBuilder::new(pattern).literal_separator(true).build()` and
combine into a `GlobSet`.

**Manifest load time:**
1. Expand placeholders to absolute paths.
2. Reject patterns containing `..` segments outright. No legitimate
   use case; allowing creates ambiguous semantics.
3. Compile each into a `Glob` with `literal_separator(true)`; combine
   into a `GlobSet`.

**Per fs request:**
1. Reject the requested path string if it contains `..`, `.`, or
   double-slash segments (i.e. require already-canonical input).
2. `std::fs::canonicalize` the path — this resolves symlinks.
3. Match the **canonicalized** result against the `GlobSet`. If it
   doesn't match, reject with `permission-denied`.

The critical invariant: matching happens **after** symlink
resolution. A symlink located inside an allowed pattern that points
outside the allow set is rejected because the resolved path no
longer matches. This is what makes "follow symlinks" safe.

TOCTOU between `canonicalize` and `read` is acknowledged and
accepted — our threat model is a user-installed gadget reading
local files, not a hostile-attacker-with-write-access scenario.

#### Symlink policy

**Follow symlinks**, with the post-canonicalization re-check
described above. The combined rule: a path is readable iff its
fully-resolved real path matches the manifest allowlist.

#### WIT sketch

```wit
interface fs {
    variant fs-error {
        permission-denied,
        not-found,
        io(string),
    }

    record file-metadata {
        size: u64,
        modified-unix-ms: u64,
        is-symlink: bool,
    }

    read-file: func(path: string) -> result<list<u8>, fs-error>;
    file-exists: func(path: string) -> bool;
    file-metadata: func(path: string) -> result<file-metadata, fs-error>;
}
```

`file-exists` returns `false` for both "absent" and "denied" — the
auth-token resolver walks a candidate list and only cares "can I use
this path?". Settled: boolean is sufficient.

#### File watching

Out of scope for v1. The merge-on-activation policy for
`saved_networks.json` removes the only known reason to need it. If a
future gadget needs change notifications, add a separate
`fs-watch` interface then.

## Settings UI — "Remembered networks" panel

The gadget's settings page exposes a management surface for the
gadget-owned history table:

- **List view** of every row in the history table — network ID, nice
  name, last-seen timestamp, last observed status.
- **Per-row "Forget"** action — deletes the row. No API call. Confirms
  with a small dialog only for currently-joined networks (which would
  reappear on next refresh anyway, but the user should know that).
- **"Clear all"** action — empties the history table. Destructive,
  confirmation dialog required.
- **"Re-import from ZeroTier UI"** action — manually triggers the
  `saved_networks.json` merge logic ad-hoc. Visible only on macOS, and
  only when the file is readable. Useful when the user added networks
  via the official UI mid-session.

**Not** in settings:
- "Add by ID without joining" — too niche, creates rows with no
  observed status / snapshot, and duplicates the main panel's
  "Join network" entry point. Joining is the canonical way to enter
  a new network ID.

## Open questions

All previously-listed open questions are now settled. Remaining
items move to implementation-time decisions:

- Final wording / iconography of state badges (UX polish).
- Whether the keyword-prefixed partial-ID Intent B variant
  (`zt <prefix>` for prefixes shorter than 16 chars) is worth
  implementing on top of the bare-16-char trigger.

## Tests / docs (to be filled in once design is settled)

- API client: unit tests against canned JSON fixtures for each
  `Network` status variant; integration test gated on `zerotier-one`
  being installed.
- Auth resolver: per-OS path resolution tests with tempdirs;
  permission-error fallback paths; manual-paste override path.
- History store: round-trip + migration tests; `saved_networks.json`
  importer tests against fixtures (including the sample captured
  during this discussion); merge-on-activation idempotence test.
- State model: tests for the three-way classification (connected /
  offline-joined / known-only) given combinations of live + history.
- fs WIT layer: placeholder expansion per OS, glob match against
  canonicalized paths, `..` rejection at manifest load and at request
  time, symlink-resolution-then-recheck path, denied-vs-not-found
  conflation in `file-exists`.
- Documentation: ADR for the integration shape (gadget, sqlite
  storage, auth UX), plus the file-access WIT layer's own ADR
  (capability model, placeholder set, v1-scope boundaries).
