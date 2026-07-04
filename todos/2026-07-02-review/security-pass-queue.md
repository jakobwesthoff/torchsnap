# Security pass — COMPLETE (2026-07-02)

Every parked observation from the deferred security pass has been
analyzed (each with an independent Fable advisor consult) and written
up as a self-contained todo. The queue of pending work is empty. This
file is now the completion index.

The single remaining item is an explicit exclusion, not pending work:

- `src-tauri/src/wasm/path_safety.rs` — excluded from review entirely
  (flagged area, per Jakob's direction); **do not analyze**. Still
  outstanding as a deliberate coverage gap, recorded in `README.md`.

## Findings produced

Severities are the reviewer's post-consult assessment.

New todos:

- `host-wasm/01kwh4j9bptrayf451yzd2145b-protocol-cors-origin-reflection.md`
  — CORS Origin reflection on the gadget scheme. **low** (hardening).
- `host-core/01kwh4j9bptrayf451yzd2145d-command-binary-path-resolution-hijack.md`
  — non-absolute command binary resolved via attacker PATH. **high**.
- `host-core/01kwh4j9bptrayf451yzd2145e-command-cwd-unvalidated.md`
  — guest `cwd` unvalidated + per-rule `cwd` dead (B4+B8). **critical**.
- `host-core/01kwh4j9bptrayf451yzd2145f-command-env-override-injection.md`
  — env overrides bypass the filter (`LD_PRELOAD`), leaky denylist.
  **critical** (threat-model-gated) + independent **high**.
- `host-wasm/01kwh4j9bptrayf451yzd2145g-archive-decompressed-size-unbounded.md`
  — zip-bomb / unbounded prealloc, host boot-loop. **medium**.
- `host-wasm/01kwh4j9bptrayf451yzd2145h-search-span-logs-query.md`
  — full query recorded in the ring-buffer log. **low–medium**.
- `host-wasm/01kwh4j9bptrayf451yzd2145j-compile-cache-unsafe-deserialize.md`
  — `unsafe deserialize_file` of an unauthenticated `.cwasm`;
  command-grant self-plant → host RCE. **high**.
- `host-core/01kwh4j9bptrayf451yzd2145k-fs-existence-oracle.md`
  — three-way fs existence/enumeration oracle + a symlink-escape leak.
  **low**.
- `host-core/01kwh4j9bptrayf451yzd2145m-http-cap-redirect-tls-body.md`
  — HTTP SSRF (redirects + DNS rebind), insecure_tls, unbounded body,
  error classification. **high**.
- `host-wasm/01kwh4j9bptrayf451yzd2145n-opener-path-scheme-unconstrained.md`
  — opener `open_path`/`open_url` unconstrained. **medium** (high chained).
- `host-core/01kwh4j9bptrayf451yzd2145p-website-metadata-ssrf-cors.md`
  — gadget-driven website-metadata SSRF + CORS echo + thread-per-request.
  **medium**.
- `host-core/01kwh4j9bptrayf451yzd2145q-gadget-install-uninstall-trust.md`
  — install/uninstall trust chain; builtin-id shadowing loader gap.
  **low**.
- `platform/01kwh4j9bptrayf451yzd2145r-app-launcher-forged-entry-id.md`
  — app-launcher execute proxy (entry-store-gated). **low**.
- `frontend/01kwh4j9bptrayf451yzd2145s-gadget-css-scope-not-boundary.md`
  — gadget CSS `@scope` is not a boundary (`}`-breakout). **low**.
- `build/01kwh4j9bptrayf451yzd2145t-csp-null-asset-scope.md`
  — null CSP + asset-protocol scope over app-data (cross-gadget read).
  **high**.
- `host-core/01kwh4j9bptrayf451yzd2145v-control-socket-no-auth.md`
  — control socket no peer auth + socket steal + no single-instance.
  **low**.
- `host-wasm/01kwh4j9bptrayf451yzd2145w-directory-source-manifest-symlink-follow.md`
  — `DirectorySource::open` reads `manifest.toml` unguarded (bonus
  finding surfaced during the B6 analysis). **low**.

Extended existing todos:

- `host-wasm/01kwfz4kkaq7spwnm2ncket1fv-protocol-no-percent-decoding.md`
  — added the guard-rails the future percent-decoding fix must satisfy
  (no live traversal today). **informational**.
- `host-wasm/01kwfz4kkaq7spwnm2ncket1g1-directory-read-fallback-raw-path.md`
  — added the TOCTOU security assessment (race-only, DirectorySource-only).
  **low**.
- `host-core/01kwg168a5spvs73hj63rfy3tw-fs-pattern-metachar-injection.md`
  — added the exploitability assessment (not gadget-controllable).
  **nil / informational** for security.
