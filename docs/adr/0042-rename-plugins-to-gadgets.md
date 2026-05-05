# 42. Rename plugins to gadgets

Date: 2026-05-05

## Status

Accepted

Amends [8. Use two-tier result display with plugin-claimed views](0008-use-two-tier-result-display-with-plugin-claimed-views.md)
Amends [9. Use multi-action model with action palette for result entries](0009-use-multi-action-model-with-action-palette-for-result-entries.md)
Amends [10. Use keyboard-first interaction model with frecency ranking](0010-use-keyboard-first-interaction-model-with-frecency-ranking.md)
Amends [11. Use Tauri channels for streaming search results from catalog and query plugins](0011-use-tauri-channels-for-streaming-search-results-from-catalog-and-query-plugins.md)
Amends [12. Use prefix-based exclusive routing for query plugins](0012-use-prefix-based-exclusive-routing-for-query-plugins.md)
Amends [13. Allow plugins to provide custom UI components for the result area](0013-allow-plugins-to-provide-custom-ui-components-for-the-result-area.md)
Amends [14. Pass app handle to plugin setup](0014-pass-app-handle-to-plugin-setup.md)
Amends [15. Add teardown to plugin lifecycle](0015-add-teardown-to-plugin-lifecycle.md)
Amends [16. Allow plugins to stream live updates to their frontend component](0016-allow-plugins-to-stream-live-updates-to-their-frontend-component.md)
Amends [17. Formalize plugin ID as canonical identifier for all subsystems](0017-formalize-plugin-id-as-canonical-identifier-for-all-subsystems.md)
Amends [18. Provide per-plugin storage via SqlStorage and FileStorage](0018-provide-per-plugin-sqlite-storage-via-plugin-store-abstraction.md)
Amends [19. Provide file-based blob storage for plugins via FileStorage](0019-provide-file-based-blob-storage-for-plugins-via-filestorage.md)
Amends [21. Extend SearchResponse with inline UI and structured variants](0021-extend-search-response-with-inline-ui-and-structured-variants.md)
Amends [22. Use named view resolution for plugin UI components](0022-use-named-view-resolution-for-plugin-ui-components.md)
Amends [23. Call all query plugins regardless of prefix registration](0023-call-all-query-plugins-regardless-of-prefix-registration.md)
Amends [24. Unify CatalogPlugin and QueryPlugin into a single Plugin trait](0024-unify-catalogplugin-and-queryplugin-into-single-plugin-trait.md)
Amends [25. Host-managed plugin enable/disable lifecycle](0025-host-managed-plugin-enable-disable-lifecycle.md)
Amends [26. Use CoalescingDispatcher for serialized settings dispatch](0026-use-coalescing-dispatcher-for-serialized-settings-dispatch.md)
Amends [27. Use string-based icon identifiers instead of React component references](0027-use-string-based-icon-identifiers-instead-of-react-component-references.md)
Amends [28. Plugin component contract via context and grouped hooks](0028-plugin-component-contract-via-context-and-grouped-hooks.md)
Amends [29. WASM plugin settings API](0029-wasm-plugin-settings-api.md)
Amends [30. WASM plugin messaging API](0030-wasm-plugin-messaging-api.md)
Amends [31. WASM plugin SQL storage API](0031-wasm-plugin-sql-storage-api.md)
Amends [32. WASM plugin scheduled tasks via host-managed cron scheduler](0032-wasm-plugin-scheduled-tasks.md)
Amends [33. WASM plugin bridge owns instance lifecycle with compile-at-load and instantiate-on-enable](0033-wasm-plugin-bridge-owns-instance-lifecycle-with-compile-at-load-and-instantiate-on-enable.md)
Amends [34. Custom title bar for auxiliary windows with native controls hidden](0034-custom-title-bar-for-auxiliary-windows-with-native-controls-hidden.md)
Amends [35. Plugin distribution via bundled and user-installable archives](0035-plugin-distribution-via-bundled-and-user-installable-archives.md)
Amends [36. Plugin trust model and deferred signing](0036-plugin-trust-model-and-deferred-signing.md)
Amends [37. WASM plugin opener API](0037-wasm-plugin-opener-api.md)
Amends [38. WASM plugin HTTP API](0038-wasm-plugin-http-api.md)
Amends [39. WASM plugin assets API](0039-wasm-plugin-assets-api.md)
Amends [40. WASM plugin command API](0040-wasm-plugin-command-api.md)
Amends [41. ZeroTier plugin architecture](0041-zerotier-plugin.md)

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
