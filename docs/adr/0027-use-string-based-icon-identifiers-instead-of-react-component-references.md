# 27. Use string-based icon identifiers instead of React component references

Date: 2026-04-05

## Status

Accepted

## Context

The frontend plugin registry, settings sidebar, and settings section headers
all used `ComponentType<SVGProps<SVGSVGElement>>` for icon props — direct
references to HeroIcon React components. This approach had three problems:

1. WASM plugins, whose manifests are JSON, cannot express a React component
   reference. They had no way to specify an icon at all.
2. Every registration site had to import the specific HeroIcon components it
   used, spreading icon imports across many files.
3. Native and WASM plugins required different code paths for icon resolution,
   adding complexity to any component that renders plugin icons.

The registry's `settingsIcon` field was also a duplicate of `icon` — they
were always set to the same value.

## Decision

Replace component references with string identifiers following a prefix
protocol. A shared `<Icon>` component (`src/components/Icon.tsx`) handles
resolution and rendering. All consumers pass strings:

| Prefix | Resolution |
|---|---|
| `heroicons:<name>` | Resolved at runtime to the corresponding `@heroicons/react` component |
| `emoji:<character>` | Rendered as a `<span>` containing the character |
| `data:<url>` | Rendered as an `<img>` with the data URL as its `src` |
| `asset:<path>` | Rendered via Tauri's `convertFileSrc` as an `<img>` |

The `settingsIcon` field is removed; `icon` alone is used throughout the
registry.

## Consequences

- WASM plugin manifests specify icons as strings (e.g.,
  `"heroicons:hand-raised"`), and they render identically to native plugins.
- No HeroIcon imports at registration sites — just a string literal.
- `<Icon>` is the single place that imports `@heroicons/react`. Adding support
  for a new icon source (e.g., plugin-bundled asset files) requires changing
  only `<Icon>`, not any consumer.
- The `settingsIcon` field is removed from `PluginRegistryEntry`. Any existing
  usage must migrate to `icon`.
