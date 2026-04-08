# Frontend Reception and Rendering

## useSearch Hook

`src/launcher/hooks/useSearch.ts`

### Channel Management

Each query change creates a new `Channel<SearchMessage>`. The previous
channel's `onmessage` is set to a no-op for cleanup. Messages are processed
via the channel callback.

### Generation Tracking

A `generationRef` counter increments on each effect run. Every incoming message
checks `generationRef.current !== generation` and discards stale messages from
superseded queries.

A separate `viewRefGenerationRef` records which generation last wrote view
refs. This ref is currently **written but never read** — it appears to be
vestigial infrastructure from an earlier design.

### View Ref Update Logic

The first message of a new generation unconditionally overwrites both
`customPluginView` and `inlinePluginView` (even if null). This handles
prefix-to-non-prefix transitions cleanly. Subsequent messages within the same
generation use "first non-null wins" semantics, so catalog results (which
carry null view refs) don't clobber a pending inline view from a query plugin.

### Incremental Merge

Incoming `ScoredEntry` arrays are merged into an accumulator via
`sortedMerge()` using binary-search insertion. This relies on both the backend
and frontend sharing the same sort comparator.

### Query Change Synchronization

Uses a `prevQuery` / `setPrevQuery` "derived state during render" pattern to
trigger `setLoading(true)` synchronously on the same render as the query
change. This avoids a flash of the previous loading state. Empty query changes
synchronously reset all state including view refs.

## Launcher Component

`src/launcher/Launcher.tsx`

### Dual Query State

Two query strings are maintained:
- `displayQuery` — shown in the input field
- `searchQuery` — drives `useSearch`

`setQuery()` updates both; `setDisplayQuery()` updates only the display. This
lets plugins visually modify the query without triggering a search re-run.

Known gap: `handleGoBack` clears both to empty string instead of restoring the
pre-plugin query state (documented TODO).

### Execute vs Search Plugin Views

Two independent sources of custom plugin UI:

1. **`searchPluginView`** — from search results (`PluginViewRef` with explicit
   view name)
2. **`executePluginView`** — from `execute()` returning `ShowCustomUI` (bare
   plugin ID string)

These are merged at render time:
```ts
const customPluginView = executePluginView
  ? { pluginId: executePluginView, view: "default" }
  : searchPluginView;
```

The string `"default"` is a magic constant — not declared in types or the
registry.

### Five Rendering States

The render output is an if/else chain:

1. **Measurement dummy** — first render to report dimensions to backend
2. **Empty** — no results, no content section
3. **Plugin custom UI** — `PluginViewContainer` mounts the registered React
   component; standard result list is hidden
4. **Inline + list** — `InlineViewContainer` above `ResultList`; inline view
   participates in keyboard navigation at index 0
5. **List only** — `ResultList` only

### PostAction Handling

`PostAction` arrives as a raw string from Rust serialization. Matched via bare
string equality (`"Dismiss"`, `"ShowCustomUI"`). No TypeScript union type
exists. `"Nothing"` and `"KeepOpen"` are unhandled — `KeepOpen` accidentally
does the right thing (nothing) but is not type-checked.

## Plugin View Registry

`src/plugins/registry.ts`

Dynamic `registry` map associates plugin IDs with their component bundles:

```typescript
interface PluginRegistryEntry {
  label: string;
  description?: string;
  icon?: string;   // e.g. "heroicons:clipboard-document-list"
  views?: Record<string, ComponentType<PluginViewProps>>;
  inlineViews?: Record<string, ComponentType<InlineViewProps>>;
  settings?: ComponentType<PluginSettingsProps>;
}
```

View and settings components are lazy-loaded via `launcherComponent()` /
`settingsComponent()` wrappers. The `PluginViewRef.view` field from the
backend is the key into `views` / `inlineViews`.

## Plugin Component Contract (ADR 0028)

Plugin components — view, inline, and settings — receive **only**
per-render data through their props (`results`, `data`, `query`,
`matchedPrefix`, `selected`). Everything else flows through the React
context defined in `src/contexts/`.

### Provider placement

`PluginViewContainer` and `InlineViewContainer` in
`src/launcher/Launcher.tsx`, plus `PluginSectionContent` in
`src/settings/SettingsPanel.tsx`, each wrap their plugin component
mount in a `<PluginContextProvider>` carrying:

- `info`: `{ id, enabled }` — identity plus reactive enabled flag
  read from `enabled.<plugin-id>`
- `runtime`: `{ sendMessage, logger }` — capabilities the host
  provides to every plugin
- `launcher` (only inside the launcher tree): `{ goBack, dismiss,
  onExecute, onFooterChange, setDisplayQuery, mouseActiveRef }`

The provider also drives the legacy `LoggerContext` so any code
reading via `useLogger()` continues to work unchanged.

### Hooks

Plugin components — and any sub-component nested arbitrarily deep —
read what they need via four hooks (in `src/contexts/`):

- `usePluginInfo()` — info slice. Available everywhere.
- `usePluginRuntime()` — runtime slice. Available everywhere.
- `useLauncher()` — launcher slice. Throws if called outside the
  launcher tree (settings panels have no launcher actions).
- `usePluginSetting<T>(key)` — reactive accessor for the active
  plugin's namespace. Returns `[value, setValue]`. The plugin id is
  derived from the surrounding provider, so call sites only deal
  with short relative key names.

### WASM plugin SDK exposure

Out-of-tree WASM plugins reach the same hooks via the
`@torchsnap/plugin-sdk/hooks` subpath, which resolves at build time
to `plugin-sdk/src/shims/hooks.ts`. The shim re-reads from
`window.__torchsnap.hooks`, populated by the host's `initPluginSdk()`
in `src/lib/sdk.ts`.

This is the same `window.__torchsnap` shim mechanism already in use
for `react` and `react/jsx-runtime`. The host source files are the
single source of truth; plugin bundles ship import statements only.

### Why not props?

The props-based contract that preceded ADR 0028 forced every
sub-component to thread `pluginId`, `usePluginSetting`, `sendMessage`,
`logger`, and the launcher actions explicitly through every level.
Adding a new ambient capability was a breaking change for every
plugin. Per-render data still travels as props because hoisting it
into context would invalidate the context value on every keystroke
and force every consumer to re-render.
