# Add `website-metadata` host WIT interface

## Context

WASM plugins have no access to the host's `WebsiteMetadataService` (favicon
lookup, page title extraction). The bangs and open-url native plugins use it
for favicon enrichment. Without this interface, converted WASM plugins must
ship without favicons or re-implement the lookup logic themselves (duplicating
host infrastructure and bypassing the shared cache).

This interface should be guarded by the same permission-declaration system as
`http` and `opener`: deny-by-default, explicit opt-in via `[permissions]` in
`manifest.toml`.

## Proposed WIT sketch

```wit
/// Per-domain website metadata — favicon URL and display name.
/// Delivered by the host's existing `WebsiteMetadataService` so
/// requests are deduplicated and cached across plugins.
interface website-metadata {
    record site-metadata {
        /// Absolute URL of the best available favicon, or `none`
        /// if no favicon could be determined.
        favicon-url: option<string>,
        /// Human-readable site name from Open Graph or `<title>`,
        /// or `none` if unavailable.
        site-name: option<string>,
    }

    /// Fetch metadata for `url`. The host resolves the origin and
    /// queries the shared metadata service. The call may block
    /// briefly on first request (network round-trip); subsequent
    /// requests for the same origin are served from cache.
    /// Returns `err(string)` if the origin is not in the plugin's
    /// declared allowlist, or on a fetch/parse failure.
    fetch: func(url: string) -> result<site-metadata, string>;
}
```

## Permission model

Same pattern as `http` and `opener`:

```toml
[permissions.website-metadata]
origins = ["https://google.com", "https://youtube.com"]

# or trust-all:
origins = ["*"]
```

- `origins` lists the site origins the plugin is allowed to enrich.
- Omitting `[permissions.website-metadata]` = no access (deny by default).
- The special value `"*"` opts the plugin in to querying any origin.
- Origins are normalized to `ascii_serialization()` at manifest parse time,
  consistent with `[permissions.http]`.

For plugins like bangs that may query arbitrary user-driven domains, `"*"` is
the pragmatic choice — the plugin has no fixed origin list at author time.

## Host-side implementation

- Reuse the existing `WebsiteMetadataService` (already `Arc`-shared in the
  plugin host).
- The `PluginState` holds an `Option<Arc<WebsiteMetadataService>>`, stashed by
  the bridge at `enable()` time (same lifecycle pattern as `http_client`).
- The bridge reads `[permissions.website-metadata].origins` from the manifest
  and stores them on the instance, enforcing origin checks at call time.
- The call blocks inline via `tokio::task::block_in_place` (same pattern as
  `http::fetch`) since `WebsiteMetadataService::fetch` is async.

## Security note

`"*"` is a wider grant than it sounds: a malicious plugin could use this
interface to exfiltrate data as encoded domain lookups. However:
- The host's `WebsiteMetadataService` already contacts only the declared
  URL's origin, not arbitrary endpoints.
- The permission must be explicitly declared in `manifest.toml`, which is
  audited at plugin install time.
- A future per-user permissions prompt (if added) could require user
  confirmation for `"*"`.

## Open Questions

- Should the interface expose raw favicon bytes (avoiding a second network
  round-trip in the frontend) or just the URL (simpler, lets the frontend
  cache)?
- Should `site-name` be included in v1 or deferred until a plugin actually
  needs it?

## Blocked-by / Enables

- No prerequisites (host infra already exists).
- Enables: `bangs` WASM conversion (favicon enrichment), `open-url` WASM
  conversion (site icon in results).

## References

- `src-tauri/src/network/website_metadata/` — existing host implementation
- `src-tauri/src/wasm/host/` — pattern for host import implementations
- `wit/torchsnap-plugin.wit` (will move to `plugins/plugin-sdk/wit/`) — WIT world
- `todos/wasm/*-wasm-host-http-opener-interfaces.md` — permission model precedent
