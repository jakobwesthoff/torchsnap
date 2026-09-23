# WIT filesystem docs name the manifest section `[permissions.fs]` — the parser only accepts `[permissions.filesystem]`

**Kind:** bug
**Severity:** medium
**Area:** gadgets/gadget-sdk/wit/torchsnap-gadget.wit

## Problem

The `filesystem` interface documentation in the WIT contract
tells gadget authors to declare read access under
`[permissions.fs]`, including a full TOML example
(`gadgets/gadget-sdk/wit/torchsnap-gadget.wit:334-348`):

```
/// Gadgets declare an exact-or-glob allowlist under
/// `[permissions.fs]` in `manifest.toml`. ...
///
/// ```toml
/// [permissions.fs]
/// read = [
///     "${xdg-config}/myapp/config.toml",
///     ...
```

The manifest parser has no `fs` field: the deserialized struct
field is `filesystem` with no serde rename
(`src-tauri/src/wasm/manifest/permissions/mod.rs:52-55`), so only
`[permissions.filesystem]` works. Real manifests use the long
form (e.g. `gadgets/zerotier/manifest.toml`,
`[permissions.filesystem]`).

A gadget author following the WIT docs writes `[permissions.fs]`
— depending on the parser's unknown-key handling this is either
rejected or (worse) silently ignored, leaving the gadget with no
fs grants and every `read-file` failing with
`permission-denied`. The host-wasm review previously found that
manifest parsing is lenient about unknown keys
(`todos/gadget-host/wasm/01kwfz4kkaq7spwnm2ncket1g3-manifest-leniency-hides-typos.md`),
which makes the silent-ignore outcome the likely one.

## Suggested fix

Change both mentions in the WIT doc block (prose + TOML example)
to `[permissions.filesystem]`. Grep the rest of the WIT and
`docs/` for other `permissions.fs` references while at it.
