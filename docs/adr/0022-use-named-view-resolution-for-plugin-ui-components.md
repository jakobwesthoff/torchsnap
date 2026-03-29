# 22. Use named view resolution for plugin UI components

Date: 2026-03-29

## Status

Accepted

Amends [13. Allow plugins to provide custom UI components for the result area](0013-allow-plugins-to-provide-custom-ui-components-for-the-result-area.md)

## Context

ADR 0013 established a static one-to-one mapping between a plugin ID and
its React component: each plugin registers at most one `view` and one
`settings` component in the frontend plugin registry.

With ADR 0021 introducing `InlineUI` alongside `CustomUI`, a plugin may
now need multiple distinct components — one for full custom UI, one for
inline display, and potentially others for different contexts. The
calculator plugin, for example, needs a `"history"` view (full custom UI
with history list) and a `"result"` inline view (expression result
display). A future plugin might want different full views for different
prefixes it handles.

A static one-component-per-slot model does not support this. Each new
component type would require adding another field to the registry entry
and another resolution path in the host.

Alternatives considered:

- **One `view` and one `inline` per plugin**: Simpler but inflexible.
  Cannot support a plugin with multiple view variants without
  introducing new registry fields for each case.
- **Component selection via `data` prop**: The plugin registers one
  component and uses the `data` JSON to switch rendering internally.
  Works but pushes routing logic into every component and makes the
  registry less descriptive.

## Decision

Replace the single `view` and `inline` fields in `PluginRegistryEntry`
with maps keyed by view name:

```ts
interface PluginRegistryEntry {
    label: string;
    settingsIcon?: ComponentType<SVGProps<SVGSVGElement>>;
    views?: Record<string, LazyComponent<PluginViewProps>>;
    inlineViews?: Record<string, LazyComponent<InlineViewProps>>;
    settings?: LazyComponent<PluginSettingsProps>;
}
```

The `view: String` field in `SearchResponse::CustomUI` and
`SearchResponse::InlineUI` (ADR 0021) selects which component to
render. Resolution is `registry[pluginId].views[viewName]` for
`CustomUI` and `registry[pluginId].inlineViews[viewName]` for
`InlineUI`.

### View names are always explicit

There is no implicit default. Every `CustomUI` and `InlineUI` response
must include a view name, and that name must exist in the plugin's
registry entry. This makes the mapping fully traceable from the Rust
plugin code to the React component.

### Registry examples

```ts
"emoji-picker": {
    label: "Emoji",
    views: {
        "picker": lazy(() => import("./emoji/EmojiGrid")),
    },
},
"calculator": {
    label: "Calculator",
    settingsIcon: CalculatorIcon,
    views: {
        "history": lazy(() => import("./calculator/CalculatorView")),
    },
    inlineViews: {
        "result": lazy(() => import("./calculator/CalculatorInline")),
    },
    settings: lazy(() => import("./calculator/CalculatorSettings")),
},
```

### Host-side propagation

The `PluginViewRef` struct (ADR 0021) carries the view name from the
Rust backend through to the frontend:

```rust
pub struct PluginViewRef {
    pub plugin_id: String,
    pub view: String,
    pub data: Option<serde_json::Value>,
}
```

The frontend's `PluginViewContainer` resolves the component by looking
up `registry[pluginViewRef.pluginId].views[pluginViewRef.view]` (or
`inlineViews` for inline views). If the lookup fails (unknown plugin or
view name), it renders nothing and logs a warning.

## Consequences

- Plugins can register multiple view components for different contexts
  without changes to the registry schema.
- The `view` field is a required string on `CustomUI` and `InlineUI`,
  preventing silent misresolution. A missing or mistyped view name
  produces a visible warning rather than a blank component.
- Existing plugins (emoji picker) must name their views during migration
  (e.g., `"picker"` for the emoji grid).
- The `settings` field remains singular (one settings component per
  plugin). There is no current need for multiple settings views, and
  adding a map there would be premature.
- View names are arbitrary strings chosen by the plugin author.
  Collisions are impossible since they are scoped to the plugin ID.
  No central naming convention is enforced.
