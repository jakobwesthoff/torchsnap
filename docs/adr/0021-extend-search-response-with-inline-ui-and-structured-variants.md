# 21. Extend SearchResponse with inline UI and structured variants

Date: 2026-03-29

## Status

Accepted

Amends [13. Allow plugins to provide custom UI components for the result area](0013-allow-plugins-to-provide-custom-ui-components-for-the-result-area.md)

## Context

ADR 0008 described two plugin view modes — inline (rendered above the
result list) and full (replaces the result list) — but the
implementation only supported full takeover via `SearchResponse::CustomUI`.
ADR 0013 formalized `CustomUI` as a tuple variant carrying
`Vec<ScoredEntry>`.

The calculator plugin needs an inline view: a result display rendered
above the standard result list without replacing it. This requires a new
`SearchResponse` variant. Additionally, both `CustomUI` and the new
inline variant need to carry opaque data for the frontend component
beyond just the result entries — for example, the calculator passes its
evaluated expression and result to its React component.

The existing `SearchResponse` also lacks an explicit "nothing" variant.
Plugins that have nothing to contribute return `Results(vec![])`, which
is semantically ambiguous (did the plugin run? did it find nothing? did
it choose not to participate?).

Alternatives considered:

- **Separate `inline_search()` method on QueryPlugin**: Adds a second
  search path to the trait, complicating the plugin contract. Requires
  the host to call two methods per plugin per query.
- **Keep `CustomUI` as a tuple variant and pass data via `sendMessage`**:
  Adds latency (extra IPC round-trip) and forces async initialization in
  components that could render immediately from synchronous data.

## Decision

Refactor `SearchResponse` into four variants:

```rust
pub enum SearchResponse {
    /// Plugin has nothing to contribute for this query.
    Nothing,
    /// Standard result list — host renders entries via `ResultList`.
    Results(Vec<ScoredEntry>),
    /// Plugin requests full custom UI (replaces the result list).
    CustomUI {
        view: String,
        data: Option<serde_json::Value>,
        results: Vec<ScoredEntry>,
    },
    /// Plugin requests inline UI (rendered above the result list).
    InlineUI {
        view: String,
        data: Option<serde_json::Value>,
        results: Vec<ScoredEntry>,
    },
}
```

### Variant semantics

- **`Nothing`**: Explicit signal that the plugin inspected the query and
  has nothing to contribute. The host skips it entirely — no entries, no
  view activation.

- **`Results(Vec<ScoredEntry>)`**: Standard list entries merged into the
  host's result list. No custom UI.

- **`CustomUI { view, data, results }`**: The plugin takes over the
  entire result area. The `view` field names a React component registered
  in the frontend plugin registry (see ADR 0022). `data` is optional
  opaque JSON passed to the component as a prop. `results` are `ScoredEntry`
  values the component may use. The host disables its navigation keybindings;
  the component owns all interaction.

- **`InlineUI { view, data, results }`**: The plugin's component renders
  in a slot above the standard result list. The result list remains
  visible below. `view` names the inline React component. `data` is
  opaque JSON for the component. `results` are `ScoredEntry` values merged
  into the host's result list alongside catalog entries.

### Host handling

The `SearchResult` internal type and `SearchMessage` IPC type gain
support for both view types:

```rust
pub struct PluginViewRef {
    pub plugin_id: String,
    pub view: String,
    pub data: Option<serde_json::Value>,
}

pub struct SearchResult {
    pub entries: Vec<SourcedEntry>,
    pub custom_plugin_view: Option<PluginViewRef>,
    pub inline_plugin_view: Option<PluginViewRef>,
    pub matched_prefix: Option<String>,
}
```

The host handles each variant in both the prefix-match and no-prefix
search paths:

- **Prefix match path**: All four variants are honored as declared.
- **No-prefix path**: `CustomUI` is **downgraded to `Results`** as a
  safety measure — full UI takeover is not permitted without an explicit
  prefix match. `InlineUI` is honored (this is its primary use case).

### Inline view selection model

In the frontend, the inline component slot is selectable — it
participates in the host's keyboard navigation as a special entry at
index 0. Arrow keys can select it, Enter executes its default action.
The standard result list entries shift to index 1+. This avoids
introducing a separate interaction model for inline views.

## Consequences

- Plugins gain a clean way to provide inline UI without full takeover,
  enabling the calculator plugin and future inline-style plugins.
- The explicit `Nothing` variant improves debugging and makes plugin
  intent unambiguous.
- `CustomUI` is no longer a simple tuple — existing call sites (emoji
  picker) must migrate to the struct variant with `view` and `data`
  fields.
- The `data` field eliminates the need for an extra `sendMessage`
  round-trip for initial render data, reducing first-paint latency for
  plugin components.
- The inline selection model adds complexity to the host's navigation
  system (index 0 is conditionally occupied by the inline component).
- The safety downgrade of `CustomUI` in the no-prefix path prevents
  accidental full takeover but means a plugin cannot provide full custom
  UI without registering a prefix.
