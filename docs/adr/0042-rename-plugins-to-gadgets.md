# 42. Rename plugins to gadgets

Date: 2026-05-05

## Status

Accepted

## Context

The marketing site adopted "Gadgets" as the user-facing label for plugins:
`web/src/site/Header.astro:14` changed the nav entry to "Gadgets", and
`web/src/landing/plugin-universe/PluginUniverse.astro:40–42` reads "Snappy's
gadgets." The in-repo codebase still used "plugin" everywhere, creating a
terminology split between the public brand and the implementation.

Torchsnap is pre-alpha and the only consumer of its own APIs. There is no
external ecosystem to protect, no version-bump strategy needed, and no
deprecation period required — a straight rename is viable.

Prior in-repo terminology used "plugin" across ~33 ADRs, 6 files under
`docs/Plugin-Architecture/`, ~2178 lines of `.rs` source, and dozens of
`.ts`/`.tsx` files. The full scope is captured in
`todos/plans/01kqwvtgqpvxb64eysw7pt4gdr-rename-plugins-to-gadgets/inventory.md`.

## Decision

Rename every in-repo identifier from "plugin" to "gadget" across
documentation, internal symbols, public API surface, repo layout, and tooling.

"Gadget" is the only term going forward in torchsnap. "Plugin" survives only
inside vendored Tauri-ecosystem dependency names (`tauri-plugin-store`,
`tauri-plugin-global-shortcut`, `@tauri-apps/plugin-*`) and inside Vite's own
exported `Plugin` type.

The `manifest.toml` filename is retained; only its `[plugin]` section renames
to `[gadget]`. The archive extension `.torchsnap` is unchanged — it is a
product name, not a plugin term. App-data directories rename without migration
code; existing test installations are wiped manually before first launch on the
renamed code.

The marketing site (`web/`) is unchanged in this repo — it is a separate
repository.

## Consequences

Every `Plugin*` Rust type, every `plugin*` TS identifier, every WIT
package/world/file, every Tauri command name, every settings-key prefix, and
every workspace directory now uses "gadget". The full naming map is in
`todos/plans/01kqwvtgqpvxb64eysw7pt4gdr-rename-plugins-to-gadgets/inventory.md`.

The `manifest.toml` filename is retained; its inner `[plugin]` table is renamed
to `[gadget]`.

The `define_plugin!` macro is renamed to `define_gadget!`.

Vendored Tauri-ecosystem identifiers and Vite's `Plugin` type are unchanged.

This ADR amends every prior ADR using plugin terminology — see the bidirectional
links inserted by `adrs link` below. The architectural decisions in those ADRs
are unchanged; only the terminology label is.

## References

- `todos/01kqw4kqe3g50xn77efwv71jy2-rename-plugins-to-gadgets-everywhere.md` — original trigger
- `todos/plans/01kqwvtgqpvxb64eysw7pt4gdr-rename-plugins-to-gadgets/inventory.md` — shared naming inventory
