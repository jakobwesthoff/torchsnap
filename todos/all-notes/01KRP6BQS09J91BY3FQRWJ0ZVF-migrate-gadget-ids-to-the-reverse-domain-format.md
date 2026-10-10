---
title: "Migrate gadget IDs to the reverse-domain format"
kind: refactor
component: gadget-host
tags: [api-design, migration]
---
# Migrate gadget IDs to the reverse-domain format

All bundled gadgets currently use short IDs (`calculator`, `bangs`, `emoji-picker` and so on). They should move to the canonical `app.torchsnap.*` reverse-domain format now, to establish the convention before third-party gadgets exist.

## New IDs

- `calculator` becomes `app.torchsnap.calculator`
- `bangs` becomes `app.torchsnap.bangs`
- `emoji-picker` becomes `app.torchsnap.emoji-picker`
- `open-url` becomes `app.torchsnap.open-url`
- `hello-world` becomes `app.torchsnap.hello-world`
- `template` becomes `app.torchsnap.template`

## Affected locations per gadget

- `manifest.toml` `[gadget] id`
- `bundled.toml` entries
- Settings store keys (`enabled.<id>`, `gadgets.<id>.*`)
- Storage directory (`gadget-home/<id>/`)
- Compile cache paths
- Frontend gadget registry (`registry.ts` internal registrations)
- Any hardcoded ID references in native gadgets (clipboard, app-launcher, system-preferences, system-commands)

## Migration

A migration path for existing user settings and data directories is needed. The `GadgetId` validator in `manifest.rs` must be updated to allow dots.
