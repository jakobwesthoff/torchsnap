# Think about: propagate "Gadgets" branding beyond the marketing site

## Trigger

Two changes on the marketing site introduced "Gadgets" as the user-facing label
for plugins:

- `web/src/site/Header.astro:14` — nav entry for the plugin-showcase section
  changed from "Plugins" to "Gadgets" (href anchor remains `#plugins`).
- `web/src/landing/plugin-universe/PluginUniverse.astro:40–42` — section H2
  reads "Snappy's gadgets. / Every one of them a plugin." The eyebrow at line 36
  ("The plugin universe") and the body copy still use "plugin" architecturally.

The web copy currently bridges both terms explicitly: "Gadgets" is the
user-facing display label; "plugin" is retained as the architectural/technical
referent in the same breath. User confirmed the "Gadgets" naming is growing on
them and feels unique (conversation, 2026-05-05).

## What the user wants to think about

Whether and how to propagate "Gadgets" into:

1. **Application UI** — user-facing strings in the launcher, settings panels,
   and any developer/debug tools shipped with the app.
2. **Documentation** — README, in-repo docs (e.g. `docs/`), ADRs, any
   user-facing guides or changelogs.
3. **Possibly later: code itself** — type names, module paths, public API
   symbols, manifest keys, CLI subcommands, package names.

This is a *think about it* item. No decision has been made.

## Open questions

- Where does "plugin" appear today in the app code, UI strings, docs, manifest
  format, CLI, and package names? An inventory pass is needed before any
  decision can be made.
- Is "plugin" load-bearing on a public API surface that third-party authors
  depend on (e.g., a `Plugin` trait, `plugin.toml` manifest, an SDK crate)?
  Renaming there would be a breaking change for community authors.
- Should "plugin" remain the technical/architectural term with "gadget" purely
  a user-facing label, or should one term fully replace the other across all
  layers?
- The current web copy bridges both terms ("Snappy's gadgets. Every one of them
  a plugin."). Should that bridge style be preserved elsewhere, or should
  "plugin" be dropped from user-facing contexts entirely?

## Out of scope for this todo

Actually performing any of the above changes. This todo exists only to ensure
the question is revisited deliberately.
