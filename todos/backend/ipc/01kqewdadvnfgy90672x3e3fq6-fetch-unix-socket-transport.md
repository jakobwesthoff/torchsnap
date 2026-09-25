---
kind: feature
status: deferred
---

# Gadget Fetch — Unix Domain Socket Transport

Needs heavy discussion before implementation.

Spun off from the ZeroTier integration discussion
(`01kqaqzdma111a817snbna5n8b-zerotier-one-integration.md`). Not blocking
for ZeroTier itself (which uses TCP loopback to `localhost:9993`), but
is the single most likely future extension to the fetch host import.

## Why we need it

A large class of "gadget integrates with a local daemon" use cases
expose their API over a Unix domain socket rather than TCP loopback.
Notable examples:

- **Docker** — `/var/run/docker.sock` (HTTP-over-Unix-socket, JSON API)
- **Podman** — `$XDG_RUNTIME_DIR/podman/podman.sock` (Docker-compatible HTTP API)
- **systemd / journald** — varbus / D-Bus over `/run/dbus/system_bus_socket`
- **pueue** — `$XDG_RUNTIME_DIR/pueue/pueue.sock`
- **zfs-zed**, **zrepl**, various daemon control sockets — all
  Unix-socket-only by design.

Each of these is a plausible future Torchsnap gadget. Without
socket-transport support in the fetch host import, those gadgets
cannot be implemented at all under the current WIT surface.

## When we need it

**Not yet.** No concrete gadget scoped that requires it. The trigger
for re-opening this todo:

- A specific gadget enters scoping that requires a socket transport.
- A reasonable-effort estimate puts that gadget's other prerequisites
  (UI, storage, parsing) at less than the cost of designing this
  transport — i.e. it becomes the dominant blocker.

Until then, deferring is correct: the architectural cost of adding
this later is **purely additive**. Existing TCP-loopback gadgets
continue to work unchanged regardless of which design we eventually
pick.

## Why it's complicated

Three orthogonal questions, each with multiple defensible answers:

### Q1. Where does the transport choice live in the API?

Several shapes are plausible:

- **A. URL-scheme overloading.** Recognize a synthetic scheme like
  `unix:///path/to/socket?http_path=/api/v1/info` or the more common
  `http+unix://%2Fvar%2Frun%2Fdocker.sock/v1.40/info` (URL-encoded
  socket path). Pro: one API surface, transparent to gadget code.
  Con: ugly URL encoding; "origin" semantics break (no host:port);
  custom URL parsing on the host.
- **B. Explicit transport field in `http-request`.** Add
  `transport: variant { tcp, unix(string) }`. URL field then carries
  only the HTTP-level path (e.g. `http://localhost/api/v1/info`),
  and the host actually connects to the socket path. Pro: clean
  separation of transport from request. Con: every gadget must
  set transport; URL is partially fictional.
- **C. Separate `unix-fetch` interface.** New WIT interface,
  separate import, separate permission shape. Pro: socket and HTTP
  semantics fully decoupled; each can evolve independently. Con:
  two parallel HTTP clients to maintain; gadgets choosing transport
  at runtime get awkward.
- **D. Raw stream interface (no HTTP at all).** Gadget gets a
  bidirectional byte stream and implements HTTP itself, or talks
  raw socket protocols (D-Bus, custom binary). Pro: maximally
  general — opens non-HTTP daemon protocols. Con: every gadget
  re-implements HTTP correctly, which is a famously bad idea.

**Lean (subject to discussion):** B if all daemons we care about
speak HTTP over the socket; D as a separate interface if non-HTTP
protocols (D-Bus, raw binary) become a real requirement. A and C
both have clear downsides.

### Q2. How does the permission model work?

The current `[permissions.http]` model is origin-based: `scheme +
host + port`. Unix sockets have none of those — they're a filesystem
path. Options:

- **a. Add a parallel allowlist:**

  ```toml
  [permissions.http]
  origins = ["https://localhost:2376"]
  unix-sockets = ["/var/run/docker.sock"]
  ```

  Independent declaration; explicit per-socket grants.
- **b. Reuse `[permissions.filesystem]`.** Socket paths are filesystem
  objects, so declare them there. Pro: single fs allowlist covers
  reads + sockets. Con: conflates "read this file" with "open this
  socket" — different capabilities, different threat models.
- **c. Glob the socket path** (e.g.
  `$XDG_RUNTIME_DIR/podman/podman.sock` for podman). Reuse the same
  placeholder set as the fs WIT layer. Pro: consistency. Con:
  globbing socket paths feels overgeneralized.

**Lean:** **a** — separate `unix-sockets` allowlist under
`[permissions.http]`. Keeps the conceptual cluster ("gadget reaches
out to a service") together while not conflating sockets with
file reads.

### Q3. How do we identify what protocol speaks over the socket?

If we go with shape **B** (transport field), the implicit assumption
is "HTTP over the socket". That covers Docker, podman, and most
modern daemons, but not D-Bus or custom binary protocols. Decision:

- **i. Restrict to HTTP-over-socket only in v1.** Documented;
  non-HTTP protocols out of scope. Future raw-socket interface
  separate.
- **ii. Add a `protocol: variant { http, raw }` selector.** `raw`
  returns a stream handle. Pro: covers D-Bus etc. immediately. Con:
  WIT-level streams are still a pain; same complexity as deferred
  streaming responses.

**Lean:** **i** for v1. If a D-Bus gadget is concretely scoped, that
gadget can carry the raw-stream interface as its own prerequisite.

## Library / implementation notes

- `reqwest` does **not** natively support Unix socket transport.
  `hyper-tls` and `hyperlocal` (a `hyper` adapter for Unix sockets)
  are the standard combo. This means the host would need a separate
  client path for unix-socket requests, not a unified reqwest client.
- Tokio `UnixStream` + `hyper::client::conn` is the mechanical
  path. ~100 lines of host code for the Unix path itself.
- Permission check happens against the canonicalized socket path,
  same `..`-rejection and symlink-resolution discipline as the fs
  WIT layer.
- Connection pooling per-socket is straightforward (path is the
  pool key) but probably over-engineered for v1 — open per-request,
  close on response, accept the per-call cost.

## Open questions to settle when this is re-opened

1. Pick one of A / B / C / D for transport shape.
2. Pick one of a / b / c for permission model.
3. Confirm i vs ii for protocol scope (HTTP-only vs HTTP + raw).
4. Decide whether `http-response.status` and the new `http-error`
   variants need any socket-specific additions (e.g.
   `socket-not-found`, `socket-permission-denied` distinct from the
   generic `connection-refused`).
5. Connection pooling: per-request open/close, or a host-side pool
   keyed by socket path?

## Tests / docs (when this work begins)

- Round-trip test against a fixture HTTP-over-Unix server (likely a
  small `hyper` server bound to a tempdir socket).
- Permission denial: socket path not in manifest → reject.
- Path traversal: `..` in socket path → reject at manifest load
  and at request time.
- Symlink resolution: socket-as-symlink to an undeclared real path
  → reject post-canonicalization.
- ADR documenting the transport-shape / permission-model decisions.
