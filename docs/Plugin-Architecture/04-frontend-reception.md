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
