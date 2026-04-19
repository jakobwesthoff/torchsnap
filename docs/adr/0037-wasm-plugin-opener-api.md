# 37. WASM plugin opener API

Date: 2026-04-19

## Status

Accepted

## Context

WASM plugins need a way to open URLs in the OS default handler (browser,
mail client, etc.) — the same action native plugins perform via Tauri's
`tauri-plugin-opener`. WASM plugins have no direct access to Tauri APIs,
so any URL-open capability must cross the WIT host-import boundary.

The concrete forcing function is the calculator plugin and any result-list
plugin that wants to open a URL when the user activates an action. Without
this interface, WASM plugins that produce "open in browser" results have no
path to actually execute the open.

A second operation that logically belongs here is `reveal-path` (Show in
Finder / Show in Explorer). It is deliberately deferred: no plugin currently
being converted to WASM needs it, and its implementation touches path
validation and cross-platform resolution in ways that deserve their own
scope. The WIT comment tracks this explicitly — the interface name `opener`
already reserves the namespace for the future `reveal-path` function.

## Decision

Add a new `interface opener` host import to the WIT world with a single
`open-url` function:

```wit
interface opener {
    open-url: func(url: string) -> result<_, string>;
}
```

### Permission model: scheme allowlist from manifest

Plugins declare which URL schemes they may open under `[permissions.opener]`
in `manifest.toml`:

```toml
[permissions.opener]
schemes = ["https", "http"]
```

The host enforces the allowlist at call time. Any scheme not listed returns
`err("scheme not permitted: {scheme}")` without invoking the OS opener.
Omitting `[permissions.opener]` entirely means the plugin has no `open-url`
access — deny by default. This mirrors the approach taken by the `http`
interface (ADR 0038) and keeps permission grants explicit and auditable from
the manifest alone.

### Host implementation

The host implementation lives in `src-tauri/src/wasm/runtime.rs` alongside
the other host-import trait implementations, not as a separate file. The
scheme check is performed synchronously before the Tauri opener call.

### `reveal-path` deferred

`reveal-path` (opening a directory in the OS file manager, e.g. Finder on
macOS) is the other natural member of an opener interface. It is not included
now because:

1. No plugin currently being migrated to WASM requires it.
2. Path validation (no traversal, confined to safe roots) deserves its own
   design pass.

When the app-launcher plugin is converted, `reveal-path` should be
re-evaluated and added to this interface if needed.

## Alternatives considered

* **Unrestricted `open-url`** (no manifest declaration required) — rejected.
  An unrestricted opener lets any installed plugin open arbitrary URLs,
  including `file://`, `javascript:`, or deep-link schemes that trigger
  actions in other applications. Requiring an explicit scheme allowlist keeps
  the attack surface proportional to the plugin's declared intent.

* **A URL allowlist instead of a scheme allowlist** — more granular (only
  allow `https://docs.example.com`) but impractical for launchers that open
  arbitrary user-provided URLs. Scheme-level is the right granularity for
  most plugins.

* **Bundling `reveal-path` now** — rejected. Including a half-baked
  `reveal-path` now would either result in a too-permissive implementation
  or a blocked API surface that gives no value until path validation is
  designed. Better to ship the narrow, correct interface and extend it when
  driven by a real conversion.

## Consequences

* WASM plugins can open URLs in the OS default handler, enabling the same
  "open in browser" action that native plugins expose today.
* The `[permissions.opener]` block in the manifest is the sole permission
  declaration — no runtime prompts, no separate capability file. Reviewers
  (and users) can audit what a plugin opens from the manifest alone.
* Plugins that never declare `[permissions.opener]` pay no runtime cost.
* `reveal-path` remains native-only until explicitly added to this interface.
  Plugins that need it today must stay native.
