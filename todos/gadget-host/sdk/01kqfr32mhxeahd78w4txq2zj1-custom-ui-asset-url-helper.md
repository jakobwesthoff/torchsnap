# Plugin frontend: ergonomic asset URL helper

## Context

The launcher-side `EntryIcon::AssetIcon` path now accepts
plugin-relative paths (`assets/icon.svg`) — the host bridge
rewrites them to `torchsnap-plugin://localhost/<plugin-id>/<path>`
URLs that the existing protocol scheme already serves
(`src-tauri/src/wasm/protocol.rs`, `src-tauri/src/wasm/bindings.rs`
post-`e7d9ef3`). Plugin Rust code now writes
`EntryIcon::AssetIcon("assets/icon.svg".into())` and the icon
shows up.

The same gap remains for **plugin frontends** (the React
bundles loaded via `[frontend] settings-bundle` and
`[frontend] component`). A settings panel that wants to embed
a logo / illustration / preview image from its own archive has
to construct `torchsnap-plugin://localhost/<plugin-id>/<path>`
manually, which means the plugin code has to discover its own
plugin id at runtime — not exposed via any clean SDK surface.

## What's needed

A small SDK helper that lets a plugin frontend write something
like:

```ts
import { pluginAsset } from "@torchsnap/plugin-sdk";

<img src={pluginAsset("assets/diagram.png")} />
```

…and have `pluginAsset` return the full
`torchsnap-plugin://localhost/<plugin-id>/<path>` URL.

The plugin id is the only piece of context needed. Two viable
delivery shapes:

1. **Runtime injection.** When the host mounts a plugin's
   frontend, it provides the plugin id via a known global or
   React context (`PluginContext` already exists in
   `src/contexts/PluginContext.tsx`). The SDK helper reads from
   there. Couples plugin frontends to the host's React tree
   somewhat, but `PluginContext` is already the public seam.

2. **Build-time substitution.** A Vite/Rolldown plugin
   substitutes `__TORCHSNAP_PLUGIN_ID__` (or similar) at build
   time. Each plugin's bundler knows its own id from
   `manifest.toml` — we already pipe `plugin.id` into the build
   step, so the substitution is trivial. Output bundle has the
   id baked in. No runtime context needed.

(2) is the cleaner answer for asset URLs specifically — they're
static and don't change per render. (1) only matters when the
helper needs to do something that genuinely depends on the
running app instance.

## Acceptance

- A plugin frontend can write `pluginAsset("relative/path.png")`
  and get a working URL without naming its own plugin id.
- The helper is documented in the SDK README's "asset
  references" section (alongside the analogous
  `EntryIcon::AssetIcon` Rust-side ergonomics).
- A plugin frontend that uses an absolute path or fully-
  qualified URL through the helper is rejected at
  build/compile time, mirroring the Rust-side rule
  (`bindings.rs::resolve_entry_icon` post-`e7d9ef3`).

## Out of scope

- Read-write access to the archive from the frontend (the
  protocol scheme is read-only by design).
- Cross-plugin asset references — the host bridge's existing
  rejection of `://` URLs in plugin-emitted icons documents
  the boundary; same rule should apply for frontend.

## Why now

Captured while landing the WASM bridge fix that made the
launcher-side `AssetIcon` work for plugins. The two gaps are
sister features — solving one without the other leaves plugin
authors with an inconsistent mental model ("relative paths
work in entry icons, but not in my settings panel's
`<img src>`").
