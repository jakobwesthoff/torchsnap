---
kind: refactor
status: open
---

# Migrate gadget IDs to reverse-domain format

All bundled gadgets currently use short IDs (`calculator`, `bangs`,
`emoji-picker`, etc.). These should be migrated to the canonical
`app.torchsnap.*` reverse-domain format to establish the convention
before third-party gadgets exist.

New IDs:
- `calculator` → `app.torchsnap.calculator`
- `bangs` → `app.torchsnap.bangs`
- `emoji-picker` → `app.torchsnap.emoji-picker`
- `open-url` → `app.torchsnap.open-url`
- `hello-world` → `app.torchsnap.hello-world`
- `template` → `app.torchsnap.template`

Affected locations per gadget:
- `manifest.toml` `[gadget] id`
- `bundled.toml` entries
- Settings store keys (`enabled.<id>`, `gadgets.<id>.*`)
- Storage directory (`gadget-home/<id>/`)
- Compile cache paths
- Frontend gadget registry (`registry.ts` internal registrations)
- Any hardcoded ID references in native gadgets (clipboard, app-launcher,
  system-preferences, system-commands)

Needs a migration path for existing user settings and data directories.
The `GadgetId` validator in `manifest.rs` needs to be updated to allow dots.
