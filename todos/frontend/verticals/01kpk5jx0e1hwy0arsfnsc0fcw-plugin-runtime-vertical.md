# Plugin runtime vertical (`src/plugin-runtime/`)

Consolidate all host-side plugin runtime infrastructure into
`src/plugin-runtime/`. Currently scattered across `src/lib/`,
`src/contexts/`, `src/hooks/`, and the top level of `src/plugins/`.

Distinct from `src/plugins/` which holds per-plugin UI implementations
(clipboard, bangs, emoji). This vertical is about the machinery that loads,
mounts, communicates with, and provides context for all plugins.

## Why

These files all exist for the same reason: enabling the plugin runtime
on the frontend. They are tightly coupled to each other and change
together (e.g., any WIT interface update touches loader, bridge, and
context simultaneously). Scattering them across four folders obscures
this coupling and makes the blast radius of interface changes hard to see.

## Files to move

| Current path | Target path |
|---|---|
| `src/lib/pluginComponent.tsx` | `src/plugin-runtime/pluginComponent.tsx` |
| `src/lib/pluginCss.ts` | `src/plugin-runtime/pluginCss.ts` |
| `src/lib/pluginMessage.ts` | `src/plugin-runtime/pluginMessage.ts` |
| `src/lib/sdk.ts` | `src/plugin-runtime/sdk.ts` |
| `src/plugins/wasmPluginLoader.ts` | `src/plugin-runtime/wasmPluginLoader.ts` |
| `src/plugins/registry.ts` | `src/plugin-runtime/registry.ts` |
| `src/plugins/types.ts` | `src/plugin-runtime/types.ts` |
| `src/contexts/PluginContext.tsx` | `src/plugin-runtime/PluginContext.tsx` |
| `src/contexts/PluginContextProvider.tsx` | `src/plugin-runtime/PluginContextProvider.tsx` |
| `src/contexts/usePluginInfo.ts` | `src/plugin-runtime/usePluginInfo.ts` |
| `src/contexts/usePluginRuntime.ts` | `src/plugin-runtime/usePluginRuntime.ts` |
| `src/contexts/usePluginSetting.ts` | `src/plugin-runtime/usePluginSetting.ts` |
| `src/hooks/usePluginSetting.ts` | merge with contexts version or deduplicate |
| `src/hooks/usePluginStream.ts` | `src/plugin-runtime/usePluginStream.ts` |

Note: there appear to be two `usePluginSetting.ts` files — one in `contexts/`
and one in `hooks/`. Investigate and deduplicate before moving.

## Barrel export

Add `src/plugin-runtime/index.ts` exporting:
- `PluginContextProvider` (mounted in launcher `main.tsx`)
- `usePluginInfo`, `usePluginRuntime`, `usePluginSetting`, `usePluginStream`
- `pluginComponent` (used by WASM plugin mount sites)
- Plugin types (`PluginInfo`, etc.)

Internal: `wasmPluginLoader`, `pluginCss`, `pluginMessage`, `sdk`, `registry`,
`PluginContext` — implementation details not exposed outside this vertical.

## Relationship to `src/plugins/`

After the move, `src/plugins/` contains only per-plugin UI folders:
```
src/plugins/
├── bangs/
├── clipboard/
└── emoji/
```

These per-plugin UI implementations import from `src/plugin-runtime/` for
shared types and hooks, but the runtime machinery itself lives in
`src/plugin-runtime/`.

## Known callers to update

- `src/launcher/main.tsx` — mounts `PluginContextProvider`
- `src/launcher/hooks/useControlChannel.ts` — likely uses plugin runtime
- `src/settings/PluginSettingsWrapper.tsx` — uses plugin context
- `src/settings/sections/PluginsManagementPanel.tsx` — uses plugin info hooks
- All per-plugin UI components in `src/plugins/`

## Suggested approach

1. Investigate the `usePluginSetting` duplication first — merge or clarify
2. Create `src/plugin-runtime/`, move files with `git mv`
3. Fix imports incrementally, keeping build green
4. Add barrel `index.ts`
5. Clean up now-empty locations in `src/contexts/`, `src/hooks/`, `src/lib/`

## Priority: MEDIUM-HIGH (largest scope, do after mascot + logger)
