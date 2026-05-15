# 1Password Integration: Rust Communication Vectors

Research into all available methods for communicating with a running
1Password system from Rust.

Researched 2026-05-13.

---

## Integration Path Comparison

| Path | Auth model | Rust integration | License | Maturity |
|------|-----------|-----------------|---------|----------|
| `op` CLI subprocess | Any (biometric, service acct, Connect) | `onepassword-cli` crate or raw `Command` | MIT | Stable |
| IPC via `onepassword-ipc-client` | Desktop app biometric | Official Rust crate | Apache-2.0 OR MIT | Official, but endpoints undocumented |
| Go SDK `core.wasm` in WASM runtime | Service account or desktop biometric | `wasmtime` + `wasmtime-wasi-http` | MIT | Official binary, unofficial integration |
| `libop_uniffi_core` native FFI | Service account or desktop biometric | `onepassword-sys` crate | MIT | Unofficial, unmaintained |
| Connect Server REST | Access token (JWT) | `connect-1password` crate or raw HTTP | MIT | Stable |
| SSH agent socket | Desktop app biometric | Standard Unix socket | N/A | Stable |

All listed dependencies are MPL-2.0 compatible. `corteq-onepassword`
(AGPL-3.0) exists but is license-incompatible and excluded.

---

## 1. `op` CLI Subprocess

Shell out to the `op` binary and parse JSON output. The simplest
path with the broadest auth support.

### How it works

The `op` binary communicates with 1Password's cloud servers (or a
Connect server, or the desktop app via IPC). It supports structured
JSON output on all read commands.

### Authentication methods

| Method | Env var / mechanism | Headless? | Session lifetime |
|--------|-------------------|-----------|------------------|
| Desktop app integration | `OP_BIOMETRIC_UNLOCK_ENABLED=true` | No | 10 min idle / 12 h hard |
| Manual sign-in | `OP_SESSION=<token>` from `op signin` | Yes | 30 min idle |
| Service account | `OP_SERVICE_ACCOUNT_TOKEN=ops_...` | Yes | Until revoked |
| Connect relay | `OP_CONNECT_HOST` + `OP_CONNECT_TOKEN` | Yes | Until revoked |

Service accounts cannot access Personal, Private, Employee, or
default Shared vaults. Vault access is fixed at token creation time.

### Key commands for vault reads

```
op read "op://Vault/Item/Field"           # single field value
op item get <name-or-id> --format json    # full item JSON
op item list --vault <name> --format json # list items
op vault list --format json               # enumerate vaults
```

**Performance:** `op item get` by name makes ~3 API requests
internally. Providing `--vault <uuid>` + item UUID reduces this to
1 request. The CLI spawns a cache daemon (`op daemon`) on macOS and
Linux that stores encrypted data in memory — cache hits reduce
latency from ~1.5 s to ~1.2 s per call but do not eliminate network
round-trips.

### Secret references (`op://` URIs)

```
op://<vault>/<item>/<field>
op://<vault>/<item>/<section>/<field>
```

Query parameters: `?attribute=otp`, `?ssh-format=openssh`, etc.
Environment variables in vault/item names are interpolated
(`op://$APP_ENV/credentials/password`). References are pure
pointers — safe to store in config files.

### `op run` / `op inject`

- `op run -- <cmd>` resolves `op://` references in environment
  variables, then executes a subprocess with resolved values. Creates
  a PTY and masks secrets printed to stdout/stderr.
- `op inject -i template -o output` performs template-based
  substitution in files.

### Rust integration

The `onepassword-cli` crate (MIT, v0.3.4) wraps `op` via
`std::process::Command`. Alternatively, shell out directly — the CLI
surface is stable and JSON output is well-structured.

### Rate limits (service accounts)

| Plan | Reads/hour | Writes/hour | Combined/day (account-wide) |
|------|-----------|-------------|----------------------------|
| Business | 10,000 | 1,000 | 50,000 |
| Teams | 1,000 | 100 | 5,000 |
| Families / Personal | 1,000 | 100 | 1,000 |

Personal accounts (non-service-account): ~50,000 calls/hour.

### Tradeoffs

- **Pro:** Broadest auth support; stable interface; no native
  dependencies beyond the `op` binary.
- **Pro:** Desktop app biometric integration gives the best UX —
  Touch ID / Windows Hello.
- **Con:** Process spawn overhead (~1.2–1.5 s per call including
  network); not suitable for high-frequency reads.
- **Con:** Requires `op` to be installed separately by the user.

### Sources

- https://developer.1password.com/docs/cli/
- https://developer.1password.com/docs/cli/secret-reference-syntax/
- https://developer.1password.com/docs/cli/app-integration/
- https://developer.1password.com/docs/service-accounts/rate-limits/
- https://crates.io/crates/onepassword-cli

---

## 2. IPC via `onepassword-ipc-client`

Official Rust crate published by 1Password for communicating with
the desktop app over the platform's native IPC transport.

### How it works

The 1Password desktop app exposes IPC endpoints. The
`onepassword-ipc-client` crate (Apache-2.0 OR MIT) implements the
client side of this protocol.

**Transport by platform:**

| Platform | Transport |
|----------|-----------|
| macOS | Mach ports |
| Linux | Abstract Unix domain sockets |
| Windows | Named pipes |

### Wire protocol

Messages use length-delimited framing (Linux/Windows): 4-byte
native-endian length prefix + payload. Maximum frame: 1,048,576
bytes (1 MB).

All platforms use the same chunking layer:

| Field | Size | Description |
|-------|------|-------------|
| Header | 1 byte | `0x01` = last chunk, `0x02` = more follow |
| Payload | 0–499,999 bytes | Chunk data |

Messages <= 499,999 bytes are a single chunk. On macOS, the client
must send a dummy (empty) chunk after each intermediate response
chunk to prompt the server for the next one; Linux and Windows push
all chunks without prompting.

### Authentication / security model

- Each new connecting process triggers a biometric/password
  authorization prompt via the desktop app.
- Authorization is per-account, per-process.
- 10-minute inactivity timeout, 12-hour hard limit.
- The desktop app retrieves the connecting process's PID for display
  in the authorization prompt.
- Credentials never leave the 1Password app process.

### Documented vs. undocumented endpoints

The README explicitly warns:

> These features, and others, use **undocumented IPC endpoints**
> which may change their structure or behavior at any time. No
> support for these will be provided. This library's functionality
> should only be used with documented integration points which
> prescribe use of the library as the expected entrypoint.

The crate does not enumerate endpoint names — code examples use the
placeholder `"your_endpoint_name"`. The documented integration
points are those used by the official SDKs for desktop app
authentication. The SDK wrapper code is open-source, so the endpoint
names could be extracted from it, but 1Password's position is that
only "documented integration points" are supported.

### Rust integration

```toml
[dependencies]
onepassword-ipc-client = { git = "https://github.com/1Password/onepassword-ipc-client" }
```

Public API: `send_to()` for one-shot connect/send/receive/disconnect;
platform-specific variants for persistent connections.

### Tradeoffs

- **Pro:** Official 1Password crate; native Rust; no subprocess
  overhead; biometric auth gives good UX.
- **Pro:** Lowest latency path — direct IPC, no HTTP, no CLI spawn.
- **Con:** The IPC endpoint names and request/response message
  formats are not publicly documented. Using this crate directly
  means tracking what the official SDKs do and accepting breakage
  risk.
- **Con:** Requires the 1Password desktop app to be running and
  unlocked. No headless/service-account path.

### Sources

- https://github.com/1Password/onepassword-ipc-client (README)

---

## 3. The Closed-Source Rust Core (`libop_uniffi_core`)

All three official 1Password SDKs (Go, Python, JavaScript) are thin
generated wrappers around a single Rust core library. The Rust
source is **closed-source** — not published on GitHub, crates.io, or
anywhere else. Only pre-compiled binaries are distributed, embedded
inside the SDK packages. All SDK packages (wrapper code and
pre-compiled binaries alike) are **MIT licensed** with no separate
EULA or redistribution restrictions.

### Architecture

```
Application code (Go / Python / JavaScript)
        ↓ Generated language bindings
libop_uniffi_core  (closed-source compiled Rust)
        ↓ IPC (desktop auth) or direct TLS (service account)
1Password desktop app  OR  1Password cloud servers
```

The core library is a "headless 1Password app" — the same Rust
codebase that powers the 1Password desktop and mobile clients,
compiled without a UI layer. It contains all business logic,
cryptography (via the `ring` crate), vault decryption, server
communication, and secret reference resolution. The language-specific
SDK layers are code-generated boilerplate with zero domain logic.

### API surface

The entire public API across all SDK targets is four functions:

- `init_client(config) -> client_id` — creates an authenticated session
- `invoke(params) -> result` — async dispatch of any named operation
- `invoke_sync(params) -> result` — synchronous dispatch
- `release_client(client_id)` — frees client memory

All domain operations (item CRUD, vault listing, secret resolution,
password generation) are serialized as JSON and dispatched through
`invoke`/`invoke_sync`.

SDK version as of 2026-02: v0.4.0 (still pre-1.0; breaking changes
between minor releases are possible).

### Per-SDK compilation targets

| Language | Package | Binary format | Binding mechanism | Size |
|----------|---------|--------------|-------------------|------|
| Python | `onepassword-sdk-python` | Native `.dylib`/`.so`/`.dll` | Mozilla UniFFI | ~18–20 MB |
| JavaScript | `onepassword-sdk-js` | `core_bg.wasm` | `wasm_bindgen` | ~9.87 MB |
| Go | `onepassword-sdk-go` | `core.wasm` | Extism (wazero) | ~9.06 MB |
| **Rust** | **None** | — | — | — |

The Go SDK also has a CGO path: when CGO is available and the
1Password desktop app is installed, it can use a native IPC client
library (`libop_sdk_ipc_client`) from the desktop app instead of the
WASM binary.

### The JS and Go WASM binaries are different

Despite both being compiled from the same Rust source, the JS and Go
SDKs ship **different WASM binaries** with fundamentally different
host requirements.

**JS SDK (`core_bg.wasm`, ~9.87 MB) — `wasm_bindgen`-based:**

Imports 60+ host functions that expect a full JavaScript engine:
Fetch API (request/response construction, headers, abort
controllers), `crypto.getRandomValues`, `Date.now()`, `setTimeout`,
`Promise`, `JSON.parse`/`stringify`, `Symbol.iterator`,
`Reflect.get`/`has`, and browser/Node.js global detection (`window`,
`self`, `globalThis`, `WorkerGlobalScope`, `navigator`,
`module.require`). **Not usable outside a JavaScript engine.**

**Go SDK (`core.wasm`, ~9.06 MB) — Extism/WASI-based:**

Requires only **4 trivial host functions** across three namespaces:

| Namespace | Function | Signature | Purpose |
|-----------|----------|-----------|---------|
| `op-extism-core` | `random_fill_imported` | `(len: i32) -> i64` | Write `len` crypto-random bytes to memory, return pointer |
| `op-now` | `unix_time_milliseconds_imported` | `() -> i64` | Current time in milliseconds |
| `zxcvbn` | `unix_time_milliseconds_imported` | `() -> i64` | Same, for the zxcvbn password-strength library |
| `op-time` | `utc_offset_seconds` | `() -> i64` | Local timezone offset in seconds |

Networking is handled via **WASI HTTP** (built into the Extism
runtime). The Go SDK configures an allowed-hosts list
(`*.1password.com`, etc.) that gates outbound HTTP. WASI HTTP is
supported by `wasmtime-wasi-http`.

### Obtaining the core for use from Rust

The library is not distributed standalone. Three approaches:

1. **Go SDK's WASM binary** (`core.wasm`, ~9.06 MB) loaded into
   `wasmtime` with `wasmtime-wasi-http`. Requires implementing only
   the 4 host functions above. Avoids platform-specific native
   binaries entirely. This is the most viable path.

2. **Python SDK's native library** extracted from the PyPI wheel.
   The `corteq-onepassword` crate automates this at build time
   (download wheel, verify SHA256, extract `.so`/`.dylib`), but it
   is AGPL-3.0 — incompatible with MPL-2.0.

3. **`onepassword-sys`** (MIT, v0.1.1, author: `anden3`) provides
   raw native FFI bindings with companion sync/async wrapper crates.
   No source repository published, very low download counts (31–130),
   not updated past v0.1.1.

### Tradeoffs

- **Pro:** Full SDK capabilities (CRUD items, vaults, secret
  resolution, password generation) without subprocess overhead.
- **Pro:** Supports both service account and desktop app auth.
- **Pro:** The Go SDK's WASM binary runs in any WASI-capable runtime
  with 4 trivial host functions + WASI HTTP — no platform-specific
  native binaries needed.
- **Pro:** MIT licensed, including pre-compiled binaries.
  Redistribution permitted with copyright notice preservation.
- **Con:** Closed-source — no official Rust SDK exists.
- **Con:** Must extract the WASM binary from the Go SDK module (or
  the native library from the Python wheel) — unusual build
  dependency.
- **Con:** ~9.06 MB binary size overhead for the WASM path.

### Sources

- https://developer.1password.com/docs/sdks/
- https://github.com/1Password/onepassword-sdk-go
- https://github.com/1Password/onepassword-sdk-js
- https://github.com/1Password/onepassword-sdk-python
- https://crates.io/crates/onepassword-sys
- https://crates.io/crates/corteq-onepassword
- https://serokell.io/blog/rust-in-production-1password

---

## 4. Connect Server REST API

Self-hosted Docker service that caches vault data locally and exposes
a REST API. Eliminates per-request cloud round-trips after initial
sync.

### Architecture

Two Docker containers on a shared volume:

- `1password/connect-api` — REST API on port 8080
- `1password/connect-sync` — continuous sync with 1Password cloud

After initial fetch, reads are served from the local encrypted cache
with no rate limits. Only the sync container needs outbound HTTPS.

### Authentication

Access tokens are JWTs (ES256) created with `op connect token create`.
Each token is scoped to specific vaults with `READ` or `WRITE`
permission. Tokens cannot access Personal, Private, Employee, or
default Shared vaults. Vault permissions on a token are immutable —
change requires revoke + recreate. Expiration: 30, 90, or 180 days.

### REST endpoints (base: `http://localhost:8080/v1`)

| Method | Path | Description |
|--------|------|-------------|
| GET | `/vaults` | List vaults (supports SCIM `filter`) |
| GET | `/vaults/{id}` | Vault details |
| GET | `/vaults/{id}/items` | List items (supports SCIM `filter`) |
| POST | `/vaults/{id}/items` | Create item |
| GET | `/vaults/{id}/items/{id}` | Get item |
| PUT | `/vaults/{id}/items/{id}` | Full item update |
| PATCH | `/vaults/{id}/items/{id}` | Partial update (RFC 6902 JSON Patch) |
| DELETE | `/vaults/{id}/items/{id}` | Delete item |
| GET | `/vaults/{id}/items/{id}/files` | List attached files |
| GET | `/vaults/{id}/items/{id}/files/{id}/content` | Download file |
| GET | `/heartbeat` | Liveness (returns `.`) |
| GET | `/health` | Health with dependency state |

No built-in TLS — deploy behind a reverse proxy for production use.

### Rust integration

The `connect-1password` crate (MIT, v2.0.1) is a community-maintained
REST client. Alternatively, use any HTTP client (`reqwest`, `ureq`)
with the JWT bearer token.

### Tradeoffs

- **Pro:** No rate limits on cached reads; self-hosted data residency.
- **Pro:** Well-documented REST API with an OpenAPI spec.
- **Con:** Requires running Docker infrastructure. Suited for
  server/CI environments, not for consumer scenarios where each user
  manages their own 1Password account.

### Sources

- https://developer.1password.com/docs/connect/
- https://crates.io/crates/connect-1password

---

## 5. SSH Agent Socket

The 1Password SSH agent implements the standard SSH agent protocol.
Relevant only for SSH key material, not general vault items.

### Socket paths

| Platform | Path |
|----------|------|
| macOS | `~/Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock` |
| Linux | `~/.1password/agent.sock` |
| Windows | `\\.\pipe\openssh-ssh-agent` |

Configuration: `~/.config/1Password/ssh/agent.toml` (macOS/Linux).

Private keys never leave the 1Password process — the agent performs
signing operations internally and returns only signatures. Every
signing request requires user authorization.

### Tradeoffs

- **Pro:** Standard protocol; any SSH agent client library works.
- **Con:** Only exposes SSH keys, not general vault items.

### Sources

- https://developer.1password.com/docs/ssh/agent/

---

## 6. Other APIs (not relevant for vault reads)

- **Events API** — audit/usage event streaming; requires 1Password
  Business; read-only events, not secrets.
  (https://developer.1password.com/docs/events-api/)
- **SCIM Bridge** — IdP user provisioning; self-hosted Docker.
- **Users API** — partner/MSP user management; OAuth 2.0.
- **Browser extension messaging** — browser-only
  (`browser.runtime.sendMessage`); restricted to whitelisted
  extension IDs.
  (https://github.com/1Password/extension-messaging)
- **1Password Shell Plugins** — framework for CLI tool integrations
  (aws, gh, etc.); not a general API.

---

## Summary

The realistic candidates for a Rust application reading vault items
from a user's 1Password installation are:

- **`op` CLI subprocess** — broadest auth support (biometric,
  service account, Connect), stable interface, minimal code. Process
  spawn + network adds ~1.2–1.5 s per call. Requires `op` installed
  separately.

- **IPC via `onepassword-ipc-client`** — lowest latency, native
  Rust, official crate, biometric UX. The IPC endpoint names and
  message formats are not publicly documented; the crate is
  positioned as a building block for the official SDKs.

- **Go SDK's `core.wasm` in a Rust WASM runtime** — full SDK
  capabilities via the closed-source 1Password Rust core compiled to
  WASM (~9.06 MB). Needs only 4 trivial host functions + WASI HTTP,
  directly hostable in `wasmtime`. MIT licensed, redistribution
  permitted. The JS SDK's WASM binary (`core_bg.wasm`) is a
  different compilation targeting `wasm_bindgen` and is not usable
  outside a JavaScript engine.

- **Connect Server REST** — well-documented API with no rate limits
  on cached reads, but requires self-hosted Docker infrastructure.
  Suited for server/CI, not consumer desktop scenarios.

### Open questions

- Whether the official SDKs' use of `onepassword-ipc-client` is
  reproducible — i.e., whether the endpoint names and message
  formats are discoverable from the open-source SDK wrapper code.
- Whether the latency cost of `op` subprocess invocation is
  acceptable for the intended access frequency.
- Whether desktop-app-only auth (biometric) is sufficient, or
  whether a headless service-account path is also needed.
