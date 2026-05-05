# 39. WASM plugin assets API

Date: 2026-04-20

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

Several WASM plugin conversion candidates ship bundled data alongside
their code — the bangs plugin ships a 2.2 MB DDG bang database,
future language-specific helpers might ship seed dictionaries,
dictionaries, lookup tables, fallback icons, etc. Native plugins
include such data through `include_bytes!` at compile time or by
reading files from the plugin directory via `std::fs`. WASM plugins
can do neither: the WASM component carries only its own `.wasm`
binary, and the wasmtime sandbox has no filesystem access.

The existing `PluginSource` trait on the host already owns a
read-only view into the plugin's `.torchsnap` archive (or development
directory) — it's how the bridge reads the WASM binary and SQL
migration files at load time, and how the frontend `plugin://` custom
protocol serves bundled frontend bundles to the webview. The natural
shape is to re-use this plumbing for a guest-facing read capability.

The primary design question is trust. Every other WASM host
capability landed so far (opener, http, clipboard, sql) requires an
explicit `[permissions.<iface>]` section in `manifest.toml` and
returns a permission-denied variant when the plugin calls outside
its declared allowlist. Should `assets` follow the same pattern?

- Applying the permission model to assets would require declaring
  each asset path (or a glob) the plugin intends to read. For a
  bundled database the plugin already *knows* the path — the
  declaration would be redundant self-attestation.
- The security boundary for the other capabilities is the
  **external resource surface**: opener reaches the OS URL handler,
  http reaches arbitrary network origins, clipboard reaches other
  applications' buffers. Assets only reaches files already shipped
  inside the plugin's own archive — files the plugin author chose
  to bundle. There is no external-surface expansion to gate.
- The host already has a hardened path guard
  (`wasm::source::validate_plugin_path`) used on every plugin-file
  read: it rejects traversal (`..`), absolute paths, Windows drive
  letters, backslashes, NUL bytes, empty paths, and — in the
  directory backend — symlink targets that resolve outside the
  plugin root. That guard is the real security boundary, and it
  fires regardless of whether a permission declaration exists.

## Decision

Add a new `interface assets` host import to the WIT world **without a
corresponding `[permissions.assets]` section**. The interface exposes
two functions:

```wit
interface assets {
    variant assets-error {
        invalid-path(string),
        not-found,
        io-error(string),
    }

    read:   func(path: string) -> result<list<u8>, assets-error>;
    exists: func(path: string) -> result<bool, assets-error>;
}
```

Plugins call these directly without manifest opt-in. Path validation
runs host-side via `validate_plugin_path` **before** the trait call,
so the `invalid-path` variant is produced structurally (no
error-string matching). `not-found` is produced by pre-probing with
`file_exists` for `read`, and by `file_exists` returning `Ok(false)`
for `exists`. Every other failure collapses to `io-error`.

Host-side the interface is wired to the plugin's own
`Arc<dyn PluginSource>` handle, retained on the bridge (step 1a of
this change) and stashed on the `PluginState` at `enable()` next to
the other capability stashes.

## Consequences

**Positive**

- Plugins that ship bundled data (bangs, future language plugins, any
  plugin with a seed database) can load it without declaring each
  path in `manifest.toml`. Matches the developer expectation that a
  plugin's own archive is trusted.
- The one-line call site (`assets::read("data/file.json")?`) keeps
  the enable-time-load pattern ergonomic.
- Re-uses the hardened `validate_plugin_path` guard as the single
  security boundary, so any future improvement to that guard
  (e.g. new rejection categories) automatically tightens assets too.
- The interface surface is deliberately tiny. `read` returns full
  bytes per call — no cached handles, no partial reads, no seek. If
  plugins genuinely need streaming, a follow-up ADR can introduce
  resource handles; for the common "load N megabytes at enable()"
  pattern the current shape is simpler and safer.

**Negative**

- Establishes a precedent for permission-less capabilities. Every
  future capability proposal must explicitly justify which model
  applies: spatial trust (like assets — the resource is already
  shipped inside the plugin), or explicit allowlist (like opener
  / http — the resource sits outside the plugin). The two
  categories should remain distinct; mixing them would erode the
  clarity of the trust model.
- `validate_plugin_path` is now load-bearing for one more call site.
  A regression there would affect every capability that routes
  through `PluginSource`, including assets. Mitigated by the
  extensive test coverage on the guard itself (see
  `src-tauri/src/wasm/source.rs` tests for all seven rejection
  categories plus the deep-canonical symlink checks).

**Neutral**

- Bytes cross the WIT boundary in full on every `read` call, including
  the 2.2 MB bangs payload. Measured over-the-wall time is still
  sub-second even for that payload, and it only runs once per enable
  on the cold-start path — not a hot loop. Plugins that genuinely
  care about hot-path I/O should cache in guest memory anyway.

## Alternatives considered

**`[permissions.assets]` with a path allowlist.** Mirrors the opener
and http pattern — plugins declare which asset paths they intend to
read, host rejects anything outside the list. Rejected: the plugin
already ships the files it reads, so the allowlist would be
redundant self-declaration with no added security property. The
guarantee `validate_plugin_path` already provides (no reads outside
the plugin root) is the one that matters.

**Reading assets at load time and passing them via `enable()`
arguments.** Host reads every asset listed in the manifest at
`enable()` time and hands them to the guest as a struct. Rejected:
defeats the lazy-load use case (plugins that only need an asset in
certain code paths), bloats the instantiation path, and couples the
manifest schema to guest implementation details.

**Going through a broader filesystem interface (none exists yet).**
Could fold asset access into a general `filesystem` capability with
scoped path prefixes. Rejected as overkill: no current plugin needs
arbitrary filesystem access, and a future `filesystem` interface
would have completely different trust-model requirements
(explicit allowlists, host-side path rewriting).

## References

- ADR 0035 — plugin distribution via bundled and user-installable archives
- ADR 0036 — plugin trust model and deferred signing
- ADR 0037 — WASM plugin opener API (comparison: permission-gated)
- ADR 0038 — WASM plugin HTTP API (comparison: permission-gated)
