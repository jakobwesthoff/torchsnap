---
title: "Add a Unix domain socket transport to the gadget fetch import"
kind: feature
component: gadget-host
status: needs-discussion
horizon: someday
origin: discussion
---
# Add a Unix domain socket transport to the gadget fetch import

Gadgets that integrate with a local daemon often expose that daemon's API over a Unix domain socket, and the fetch host import cannot reach such a socket today. Docker, Podman, pueue and similar daemons are examples. The matter needs heavy discussion before anyone implements it. It was spun off from the ZeroTier integration discussion. ZeroTier does not need it, since it uses TCP loopback to `localhost:9993`, but this is the single most likely future extension to the fetch host import.

## Why we need it

A large class of "gadget integrates with a local daemon" use cases expose their API over a Unix domain socket rather than TCP loopback. Notable examples:

- **Docker**: `/var/run/docker.sock` (HTTP over Unix socket, JSON API)
- **Podman**: `$XDG_RUNTIME_DIR/podman/podman.sock` (Docker-compatible HTTP API)
- **systemd / journald**: varbus / D-Bus over `/run/dbus/system_bus_socket`
- **pueue**: `$XDG_RUNTIME_DIR/pueue/pueue.sock`
- **zfs-zed**, **zrepl**, and various daemon control sockets: all Unix-socket-only by design.

Each of these is a plausible future Torchsnap gadget. Without socket-transport support in the fetch host import, those gadgets cannot be implemented at all under the current WIT surface.

## When we need it

**Not yet.** No concrete gadget has been scoped that requires it.

Until then, deferring is correct. The architectural cost of adding this later is purely additive. Existing TCP-loopback gadgets continue to work unchanged regardless of which design we eventually pick.

## Revisit when

- A specific gadget enters scoping that requires a socket transport.
- A reasonable-effort estimate puts that gadget's other prerequisites (UI, storage, parsing) at less than the cost of designing this transport, which makes the transport the dominant blocker.

## Why it's complicated

Three orthogonal questions, each with multiple defensible answers.

### Q1. Where does the transport choice live in the API?

Several shapes are plausible:

- **A. URL-scheme overloading.** Recognize a synthetic scheme like `unix:///path/to/socket?http_path=/api/v1/info`, or the more common `http+unix://%2Fvar%2Frun%2Fdocker.sock/v1.40/info` (URL-encoded socket path). Pro: one API surface, transparent to gadget code. Con: ugly URL encoding, "origin" semantics break (no host and port), and custom URL parsing on the host.
- **B. Explicit transport field in `http-request`.** Add `transport: variant { tcp, unix(string) }`. The URL field then carries only the HTTP-level path (for example `http://localhost/api/v1/info`), and the host connects to the socket path. Pro: clean separation of transport from request. Con: every gadget must set the transport, and the URL is partly fictional.
- **C. Separate `unix-fetch` interface.** A new WIT interface, a separate import and a separate permission shape. Pro: socket and HTTP semantics are fully decoupled, and each can evolve independently. Con: two parallel HTTP clients to maintain, and gadgets that choose the transport at runtime get awkward.
- **D. Raw stream interface (no HTTP at all).** The gadget gets a bidirectional byte stream and implements HTTP itself, or talks raw socket protocols (D-Bus, custom binary). Pro: maximally general, and it opens non-HTTP daemon protocols. Con: every gadget re-implements HTTP correctly, which is a famously bad idea.

**Lean (subject to discussion):** B if all daemons we care about speak HTTP over the socket. D as a separate interface if non-HTTP protocols (D-Bus, raw binary) become a real requirement. A and C both have clear downsides.

### Q2. How does the permission model work?

The current `[permissions.http]` model is origin-based: scheme, host and port. Unix sockets have none of those. They are a filesystem path. Options:

- **a. Add a parallel allowlist:**

  ```toml
  [permissions.http]
  origins = ["https://localhost:2376"]
  unix-sockets = ["/var/run/docker.sock"]
  ```

  An independent declaration with explicit per-socket grants.
- **b. Reuse `[permissions.filesystem]`.** Socket paths are filesystem objects, so declare them there. Pro: a single filesystem allowlist covers reads and sockets. Con: it conflates "read this file" with "open this socket", which are different capabilities with different threat models.
- **c. Glob the socket path** (for example `$XDG_RUNTIME_DIR/podman/podman.sock` for podman), reusing the placeholder set of the filesystem WIT layer. Pro: consistency. Con: globbing socket paths feels over-generalized.

**Lean:** **a**, a separate `unix-sockets` allowlist under `[permissions.http]`. It keeps the conceptual cluster "gadget reaches out to a service" together without conflating sockets with file reads.

### Q3. How do we identify which protocol speaks over the socket?

If we go with shape B (transport field), the implicit assumption is "HTTP over the socket". That covers Docker, Podman and most modern daemons, but not D-Bus or custom binary protocols.

- **i. Restrict to HTTP over socket only in v1.** Documented, with non-HTTP protocols out of scope. A future raw-socket interface is separate.
- **ii. Add a `protocol: variant { http, raw }` selector.** `raw` returns a stream handle. Pro: covers D-Bus and similar immediately. Con: WIT-level streams are still a pain, and the complexity matches deferred streaming responses.

**Lean:** **i** for v1. If a D-Bus gadget is concretely scoped, that gadget can carry the raw-stream interface as its own prerequisite.

## Library and implementation notes

- `reqwest` does **not** natively support Unix socket transport. `hyper-tls` and `hyperlocal` (a `hyper` adapter for Unix sockets) are the standard combination. The host would need a separate client path for unix-socket requests, not a unified reqwest client.
- Tokio `UnixStream` with `hyper::client::conn` is the mechanical path. About 100 lines of host code for the Unix path itself.
- The permission check runs against the canonicalized socket path, with the same `..` rejection and symlink resolution as the filesystem WIT layer.
- Connection pooling per socket is straightforward, since the path is the pool key. It is probably over-engineered for v1: open per request, close on response, and accept the per-call cost.

## Open questions to settle when this is re-opened

1. Pick one of A, B, C or D for the transport shape.
2. Pick one of a, b or c for the permission model.
3. Confirm i or ii for protocol scope (HTTP only, or HTTP plus raw).
4. Decide whether `http-response.status` and the new `http-error` variants need socket-specific additions (for example `socket-not-found` and `socket-permission-denied`, distinct from the generic `connection-refused`).
5. Connection pooling: per-request open and close, or a host-side pool keyed by socket path?

## Tests and docs (when this work begins)

- Round-trip test against a fixture HTTP-over-Unix server (likely a small `hyper` server bound to a tempdir socket).
- Permission denial: socket path not in the manifest leads to rejection.
- Path traversal: `..` in the socket path is rejected at manifest load and at request time.
- Symlink resolution: a socket that is a symlink to an undeclared real path is rejected after canonicalization.
- An ADR documenting the transport-shape and permission-model decisions.
