---
kind: feature
status: open
---

# Gadget frontend: ergonomic asset URL helper

## Context

The launcher-side `EntryIcon::AssetIcon` path now accepts
gadget-relative paths (`assets/icon.svg`) — the host bridge
rewrites them to `torchsnap-gadget://localhost/<gadget-id>/<path>`
URLs that the existing protocol scheme already serves
(`src-tauri/src/wasm/protocol.rs`, `src-tauri/src/wasm/bindings.rs`
post-`e7d9ef3`). Gadget Rust code now writes
`EntryIcon::AssetIcon("assets/icon.svg".into())` and the icon
shows up.

The same gap remains for **gadget frontends** (the React
bundles loaded via `[frontend] settings-bundle` and
`[frontend] component`). A settings panel that wants to embed
a logo / illustration / preview image from its own archive has
to construct `torchsnap-gadget://localhost/<gadget-id>/<path>`
manually, which means the gadget code has to discover its own
gadget id at runtime — not exposed via any clean SDK surface.

## What's needed

A small SDK helper that lets a gadget frontend write something
like:

```ts
import { gadgetAsset } from "@torchsnap/gadget-sdk";

<img src={gadgetAsset("assets/diagram.png")} />
```

…and have `gadgetAsset` return the full
`torchsnap-gadget://localhost/<gadget-id>/<path>` URL.

The gadget id is the only piece of context needed. Two viable
delivery shapes:

1. **Runtime injection.** When the host mounts a gadget's
   frontend, it provides the gadget id via a known global or
   React context (`GadgetContext` already exists in
   `src/contexts/GadgetContext.tsx`). The SDK helper reads from
   there. Couples gadget frontends to the host's React tree
   somewhat, but `GadgetContext` is already the public seam.

2. **Build-time substitution.** A Vite/Rolldown gadget
   substitutes `__TORCHSNAP_GADGET_ID__` (or similar) at build
   time. Each gadget's bundler knows its own id from
   `manifest.toml` — we already pipe `gadget.id` into the build
   step, so the substitution is trivial. Output bundle has the
   id baked in. No runtime context needed.

(2) is the cleaner answer for asset URLs specifically — they're
static and don't change per render. (1) only matters when the
helper needs to do something that genuinely depends on the
running app instance.

## Acceptance

- A gadget frontend can write `gadgetAsset("relative/path.png")`
  and get a working URL without naming its own gadget id.
- The helper is documented in the SDK README's "asset
  references" section (alongside the analogous
  `EntryIcon::AssetIcon` Rust-side ergonomics).
- A gadget frontend that uses an absolute path or fully-
  qualified URL through the helper is rejected at
  build/compile time, mirroring the Rust-side rule
  (`bindings.rs::resolve_entry_icon` post-`e7d9ef3`).

## Out of scope

- Read-write access to the archive from the frontend (the
  protocol scheme is read-only by design).
- Cross-gadget asset references — the host bridge's existing
  rejection of `://` URLs in gadget-emitted icons documents
  the boundary; same rule should apply for frontend.

## Why now

Captured while landing the WASM bridge fix that made the
launcher-side `AssetIcon` work for gadgets. The two gaps are
sister features — solving one without the other leaves gadget
authors with an inconsistent mental model ("relative paths
work in entry icons, but not in my settings panel's
`<img src>`").
