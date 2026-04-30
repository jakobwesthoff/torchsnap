# Open-URL: asymmetric metadata lookup mode (`Blocking` for bare, `Cached` for explicit-scheme)

## Context

The open-url WASM port currently uses
`website_metadata::lookup_blocking` for every metadata fetch,
mirroring the native plugin's behaviour exactly. This is the
right starting point — simpler code, single mental model, and
matches the pre-WASM UX one-to-one.

There's a potential refinement worth investigating once the
port is in production and we have feel for the actual UX:
**split the lookup mode by URL kind**, because the two cases
have meaningfully different correctness contracts.

## The asymmetry

The native plugin (and the current WASM port) treats the two
input forms differently in the result-construction path:

| User typed | Native behaviour |
|---|---|
| `https://example.com` (explicit scheme) | Show result; on metadata `Unreachable`, render with fallback (domain as title, `globe-alt` icon) |
| `example.com` (bare domain) | Show result *only if* host says reachable; on `Unreachable`, **suppress entirely** so typo'd words ending in `.com` don't litter results |

The bare-domain suppression is load-bearing UX: the PSL check
catches `hello.notarealtld`, but `valid-tld-but-doesnt-exist.com`
only fails at the network layer, so the metadata lookup is
*part of result validity*, not just decoration.

For the explicit-scheme case the metadata lookup is purely
chrome (icon + nicer title) — the result renders either way.
That's the friction point: blocking on metadata when the result
is going to render no matter what is paying network latency
for icing.

## Proposed split (Option B)

```rust
let mode = if had_explicit_scheme {
    // Result will render regardless — metadata is decoration.
    LookupMode::Cached
} else {
    // Result existence depends on reachability — must wait.
    LookupMode::Blocking
};
```

For the cached/explicit-scheme path, on `Pending` or `Empty`,
fall back to the hero-icon path (mirrors bangs).

## Why deferred

- **No data yet.** We don't know whether the per-keystroke
  latency on explicit-scheme URLs is actually noticeable in
  practice. Most users typing `https://...` pause naturally,
  and the cache hit rate after the first lookup is ~100%.
- **Adds branching.** Two modes, two fallback strategies, more
  test surface. Worth it only if the UX win is real.
- **Native-parity simplicity wins for v1.** The current port
  ships behaviour identical to the pre-WASM plugin, so the
  WASM migration introduces zero behavioural risk. Option B
  changes behaviour (in a defensible direction, but still a
  change) and should be evaluated on its own merits later.

## Trigger to revisit

- Reports of perceptible lag on per-keystroke open-url
  rendering
- A general push to reduce blocking host calls in the
  search loop (e.g. if other per-keystroke plugins start
  hitting blocking host imports)
- Cache eviction policy changes that increase cold-cache
  rate (currently the in-memory cache has effectively
  infinite TTL within a session)

## References

- `plugins/open-url/src/lib.rs` — current implementation, single
  `lookup_blocking` call site
- `src-tauri/src/plugins/open_url/mod.rs` — historical native
  reference (deleted during WASM port)
- `plugins/plugin-sdk/src/website_metadata.rs` — wrapper exposing
  both `lookup_cached` and `lookup_blocking`
- `todos/01kn82bmj3y706tme2prsa7d2b-bangs-prefix-mode.md` — bangs
  uses `lookup_cached` because metadata is purely decoration there
  (the bang result exists unconditionally); useful contrast
