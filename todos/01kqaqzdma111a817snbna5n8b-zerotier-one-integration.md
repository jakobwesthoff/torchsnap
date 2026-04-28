# ZeroTier One Integration

**Status: NEEDS FURTHER DISCUSSION BEFORE IMPLEMENTATION**

Scope and design require alignment before code is written. Open questions
listed at the bottom.

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

Open question: host integration vs. dedicated plugin (see Open Questions).

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

## Auth token resolution (per OS)

The OpenAPI doc lists three canonical paths:

- **macOS** — `/Library/Application Support/ZeroTier/One/authtoken.secret`
  is `-rw------- root:wheel`, **not readable by unprivileged apps**.
  The official UI reads the **user-side copy** at
  `~/Library/Application Support/ZeroTier/One/authtoken.secret`
  (root-owned but `0644`, world-readable). Resolver must try the user
  path first, system path as fallback.
- **Linux** — `/var/lib/zerotier-one/authtoken.secret` is `0600 root:root`.
  Some distros provide a `zerotier-one` group; otherwise the user must
  provide the token (manual paste, or one-time `sudo` install of a
  user-readable copy into Torchsnap's data dir).
- **Windows** — `C:\ProgramData\ZeroTier\One\authtoken.secret`,
  admin-restricted.

The resolver must therefore not assume the canonical path is readable.
Fallback chain:

1. Torchsnap-configured override path (settings).
2. User-supplied pasted token (stored in Torchsnap's secret store).
3. OS-canonical path (read attempt — may fail with EACCES).

Surface a clear error UX when no token is obtainable.

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

Implication: Torchsnap must own its own history store. Proposed shape
(open for discussion):

```
{
  id: NetworkId,
  name: String,
  first_seen: Timestamp,
  last_seen: Timestamp,
  last_assigned_addresses: Vec<IpNet>,
  last_status: Option<NetworkStatus>,
  // optionally: cached full Network snapshot
}
```

Backing store: still TBD — sled, SQLite (consistent with existing plugin
SQL host), or a flat JSON file. Decision deferred to discussion.

### `saved_networks.json` interaction

Two reasonable modes; pick one per discussion:

- **Import-once seed (preferred default).** On first run on macOS, if
  the file exists, import every entry into Torchsnap's history. After
  that, Torchsnap is the sole writer of its own history. Simple, no
  ongoing coordination.
- **Continuous monitoring (opt-in).** If the user runs the official
  ZeroTier UI alongside Torchsnap, watch the file for changes and merge
  new entries. Adds complexity (file watcher, conflict resolution if
  both sides edit) and is only meaningful when both UIs coexist.

Torchsnap **does not** write back to `saved_networks.json` in either
mode — that file is the official UI's private cache.

## State model exposed to Torchsnap UI

For each network ID, three possible states (union of live state and
history):

- **connected** — in live `GET /network` with `status == "OK"`.
- **joined-but-offline** — in live `GET /network` with any other status.
- **known-but-not-joined** — in Torchsnap history only, not in live.

Refresh model: poll `GET /network` on UI cadence (debounce / on-open is
likely enough; no need for high-frequency polling). Status changes are
visible only via the API; there is no push/event channel.

## Action commands and their wire mappings

| User action | API operation | History side-effect |
|---|---|---|
| Connect to known network (in history, not currently joined) | `POST /network/{id}` (empty / default body) | upsert `last_seen` |
| Join brand-new network (not in history) | `POST /network/{id}` (empty body) | insert new history entry |
| Leave a currently-joined network | `DELETE /network/{id}` | keep history entry (downgrade to "known-but-not-joined") |
| Forget a known network | (no API call) | delete history entry |
| Force reconnect | `DELETE` followed by `POST` | upsert |

Critical: from the daemon's perspective, **"connect to known" and "join
new" are the same call**. The distinction is purely a Torchsnap UI/UX
label driven by whether the ID exists in history. There is no separate
"reconnect to a known network without re-joining" semantic on the wire.

## Crate / approach choice (open)

For the HTTP client itself, options previously discussed:

1. **`progenitor`** — generate a typed client from the OpenAPI YAML at
   build time. Fits a small, clean spec like this. Adds a build-time
   dep on `progenitor-impl`.
2. **`openapi-generator-cli`** — broader feature coverage but uglier
   Rust output and a Java toolchain to regenerate.
3. **Hand-rolled `reqwest` + `serde`** — only ~5 endpoints needed (one
   really, plus join/leave/status). For this surface area, hand-rolled
   is probably less code than wiring a generator and keeps the
   dependency footprint small.

Recommendation pending discussion: **hand-rolled** unless we want
broader coverage later. Decision deferred.

## Plugin-side WIT prerequisites

If this is implemented as a WASM plugin (see open question 1 below),
the following host-import surface does not yet exist or may not be
sufficient and **must be addressed before the integration itself can
be tackled**:

1. **File access WIT layer — does not exist yet.** Plugins currently
   have no way to read files from the host filesystem. ZeroTier
   integration needs:
   - Reading `authtoken.secret` from a per-OS canonical path (see auth
     section above) or from a user-configured override path.
   - Reading `saved_networks.json` (macOS only) for the import-once
     seed.

   A new host import for filesystem access is therefore a hard
   prerequisite. Scope of that layer (read-only? path allow-list?
   sandbox model? per-plugin grants?) is a separate design discussion
   and should be settled before this todo is unblocked.

2. **File watching — open design question for the file-access layer.**
   `saved_networks.json` continuous-monitoring mode (see "Continuous
   monitoring" above) needs change notifications. Two options:

   - Bake watch semantics directly into the new file-access WIT layer
     (`watch(path) -> stream<event>` style). Pro: one cohesive layer,
     no second host import. Con: pulls reactive/streaming semantics
     into what would otherwise be a simple read API.
   - Keep the file-access layer purely synchronous read/write and
     introduce a separate `host:fs-watch` import later. Pro: cleaner
     separation. Con: two imports to grant for one feature.

   Decision deferred — needs discussion when the file-access layer is
   designed. Note also that file watching is only useful if we choose
   the continuous-monitoring policy for `saved_networks.json`; if we
   stick with import-once, watching is not required at all and can be
   dropped from the file-access layer's v1 scope.

3. **Fetch WIT layer JSON validation.** ZeroTier's API is JSON-only
   for both request bodies (`POST /network/{id}` takes a `Network`
   JSON body) and responses. Before this todo is implementable we
   must verify that the existing fetch host import supports:
   - Setting arbitrary request headers (specifically `X-ZT1-Auth`
     and `Content-Type: application/json`).
   - Sending a JSON request body on `POST` (and `DELETE` without
     body).
   - Receiving and exposing JSON response bodies cleanly to the
     plugin (raw bytes are fine; plugin can serde-parse).
   - Talking to `http://localhost:9993` — i.e. plain HTTP loopback,
     not just HTTPS, and no CORS / origin restrictions that would
     block local-loopback requests.

   If any of these is missing, the fetch WIT layer must be adapted
   first. Validation task: read `runtime/host/http.rs` and the
   corresponding WIT to confirm — capture findings here before
   moving on.

## Open questions (need answers before plan)

1. **Host vs. plugin.** Does this live in the Tauri host (always
   available, surfaces in the main UI) or as a dedicated WASM plugin
   (consistent with current architecture direction, but plugins don't
   have direct filesystem / network access — would need new host
   capabilities for both auth-token reading and arbitrary HTTP to
   `localhost:9993`)?
2. **History backing store.** SQLite via the plugin SQL host?
   Standalone sled? Plain JSON file? Tied to question 1.
3. **`saved_networks.json` policy.** Import-once seed only, or opt-in
   continuous monitoring?
4. **Auth token UX.** Auto-detect with hard fail when unreadable, or
   always offer a manual paste path as first-class UX?
5. **Disconnect semantics in the UI.** Three plausible meanings of
   "disconnect" (leave entirely / forget / force reconnect cycle) —
   which one(s) does Torchsnap expose, and how are they labelled?
6. **Polling cadence.** On-open only? Background poll while the UI is
   visible? Long-lived background poll for status changes when hidden?
7. **HTTP client approach.** Hand-rolled vs. progenitor — depends partly
   on whether we expect to use more of the API in future.
8. **Multi-platform priority.** Is macOS the only target for v1, or do
   we need Linux/Windows path resolution and token-permission UX from
   the start?
9. **File-access WIT layer scope.** Read-only or read+write? Path
   allow-listing model? Per-plugin permission grants? Whether file
   watching is part of this layer or a separate one (see prerequisite
   2 above).
10. **Fetch WIT layer adaptation.** What, if anything, in the existing
    fetch host import needs to change to support arbitrary headers,
    JSON request bodies, and plain-HTTP loopback (see prerequisite 3
    above)?

## Tests / docs (to be filled in once design is settled)

- API client: unit tests against canned JSON fixtures for each
  `Network` status variant; integration test gated on `zerotier-one`
  being installed.
- Auth resolver: per-OS path resolution tests with tempdirs;
  permission-error fallback paths.
- History store: round-trip + migration tests; `saved_networks.json`
  importer tests against fixtures (including the sample captured during
  this discussion).
- State model: tests for the three-way classification (connected /
  offline-joined / known-only) given combinations of live + history.
- Documentation: ADR for the integration shape (host vs. plugin,
  storage choice, auth UX), plus user-facing docs for the Linux/Windows
  permission story.
