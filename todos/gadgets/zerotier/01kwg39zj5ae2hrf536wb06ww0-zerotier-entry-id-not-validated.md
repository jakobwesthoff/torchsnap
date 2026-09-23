# ZeroTier: `parse_entry_id` splices unvalidated input into daemon URL paths

**Kind:** improvement
**Severity:** low
**Area:** gadgets/zerotier/src/query.rs, gadgets/zerotier/src/api/client.rs

## Problem

`parse_entry_id` only strips the `network:` prefix
(`gadgets/zerotier/src/query.rs:120-122`):

```rust
pub fn parse_entry_id(entry_id: &str) -> Option<String> {
    entry_id.strip_prefix("network:").map(|s| s.to_string())
}
```

The result is spliced verbatim into the daemon URL path by
`join_network` / `leave_network`
(`gadgets/zerotier/src/api/client.rs:66-84`):

```rust
let path = format!("/network/{id}");
```

Entry ids arrive in `execute()` from the host (which round-trips
whatever the webview hands it), so a value like
`network:../controller/network/xyz` becomes
`POST http://localhost:9993/network/../controller/network/xyz` —
still confined to the daemon by the manifest origin allowlist,
but able to address *other daemon endpoints* than the two the
gadget intends (the controller API on nodes that run one is the
interesting target; `DELETE` against a controller network is
destructive). The validator that exists for query input —
`is_zt_network_id`, 16 hex chars (`query.rs:31-33`) — is not
applied on the execute path.

Whether a hostile webview can actually reach `execute()` with a
forged entry id was a host-side question tracked against the
app-launcher (done, removed 2026-09-23). Independent of that
answer, the gadget-side fix is cheap defense-in-depth.

## Suggested fix

Validate in `parse_entry_id`:

```rust
entry_id.strip_prefix("network:")
    .filter(|s| is_zt_network_id(s))
    .map(|s| s.to_ascii_lowercase())
```

All legitimate producers already emit `network:<16-hex-lowercase>`
(`entry_id`, `query.rs:113-115`), so this rejects only forged or
corrupted ids. Add a `parse_entry_id("network:../x")` → `None`
test next to the existing malformed-id cases (`query.rs:262-266`).
