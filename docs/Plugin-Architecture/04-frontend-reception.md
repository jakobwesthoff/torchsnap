# Frontend Reception and Rendering

How plugin output reaches the launcher webview, gets merged into the
result list, and is rendered as either standard rows, an inline
component above the list, or a full plugin-claimed view.

## Streaming results — `useSearch`

`src/launcher/hooks/useSearch.ts`.

Each query change opens a fresh `Channel<SearchMessage>` (Tauri's
typed channel API) and invokes the `search` command with it. The
backend pushes one `searchResults` message per source — the catalog
layer plus every query plugin — and a final `done` message. The
channel mirrors the `SearchMessage` union from `src/types.ts`, which
itself mirrors `src-tauri/src/search/mod.rs`.

### Per-source accumulator

Entries are kept in a `Map<sourceKey, SourcedEntry[]>` ref, keyed via
`resultSourceKey(message.source)`:

- `catalog` for the aggregated catalog layer (one batch covering all
  catalog-providing plugins),
- `plugin:<id>` for each query plugin.

Every incoming `searchResults` replaces that source's batch
wholesale. An empty batch evicts the source's entry from the Map —
that is how a plugin transitioning from results to empty (e.g.
`bangs` losing its trigger mid-query) disappears atomically without
explicit clear messages.

After each message the displayed list is rebuilt by flattening every
batch and re-sorting via `compareEntries`. The hook acknowledges this
is `O(n log n)` per message and notes a k-way merge as the obvious
optimisation if profiling shows it.

### Generation gate

A `generationRef` counter increments on every `useEffect` run.
Channel callbacks check `generationRef.current !== generation` and
discard messages from superseded queries. The previous channel's
`onmessage` is also reset to a no-op on cleanup.

### View ref handling

`customPluginView`, `inlinePluginView` and `matchedPrefix` each
arrive on every `searchResults` message. They follow a different
rule than entries:

- The first message of a generation overwrites unconditionally
  (including with `null`) so stale view refs from a previous query
  cannot leak through.
- Subsequent messages of the same generation use "first non-null
  wins" semantics — the catalog batch carries `null` view refs and
  must not clobber a pending inline/custom view from a slower plugin
  message.

Empty queries reset entries and view refs synchronously during
render via the `prevQuery` derived-state pattern, so plugin
components unmount on the same render and their effects cannot
re-inject stale display-query state.

The hook returns `{ results, customPluginView, inlinePluginView,
matchedPrefix, loading }`.

## `Launcher` component

`src/launcher/Launcher.tsx` orchestrates input, search, and the five
content states.

### Dual query state

Two strings:

- `displayQuery` — what the input renders.
- `searchQuery` — what `useSearch` consumes.

`setQuery()` writes both. `setDisplayQuery()` (passed into plugins
via `LauncherActions`) writes only the display, letting a plugin
adjust the visible text without retriggering search. The next user
keystroke resyncs both. `handleGoBack` currently clears both to the
empty string — there is a TODO to snapshot/restore the pre-plugin
query.

### Two custom-view sources

A plugin-claimed view can come from either:

1. **`searchPluginView`** — the `customPluginView: PluginViewRef`
   from `useSearch`, set when an enabled plugin returned `CustomUI`
   for the current query.
2. **`executePluginView`** — set locally when:
   - `search_execute` returns `PostAction::ShowCustomUI { view, data
     }` (the entry's `source` becomes the plugin id), or
   - the backend emits the `activate-plugin-custom-ui` Tauri event
     because a global shortcut handler returned `ShowCustomUI`. The
     payload carries `{ pluginId, view, data }`.

Resolution: `executePluginView ?? searchPluginView`. Both sides are
full `PluginViewRef` values with an explicit `view` name and
optional opaque `data` — there is no implicit `"default"` fallback.

### Inline view

`inlinePluginView` from `useSearch` is rendered above the result
list when no custom view is active (`customPluginView == null`).
The inline slot occupies index 0 of the keyboard navigation model
— the result list's logical index becomes `selectedIndex - 1` and
its total nav count becomes `results.length + 1`. `selected` is
threaded into the inline component as a prop so it can render its
own selected state.

### Five content states

Selected by an if/else chain in `Launcher.tsx`:

1. **Measurement dummy** — initial render reports the card's real
   dimensions to the backend before the first show.
2. **Empty** — no inline view and no results: only the search input
   renders; no content section, no footer.
3. **Plugin custom view** — `PluginViewContainer` mounts the
   registered React component. The standard list is hidden and
   keyboard navigation is disabled (`enabled: customPluginView ===
   null`).
4. **Inline + list** — `InlineViewContainer` above `ResultList`.
   Inline slot participates in nav at index 0.
5. **List only** — `ResultList` only.

### Action execution and `PostAction`

List-mode `executeEntry` invokes the `search_execute` command and
matches the returned `PostAction`:

- `"Dismiss"` — dismiss the launcher.
- `"Nothing"` / `"KeepOpen"` — explicit no-op.
- `{ ShowCustomUI: { view, data } }` — switch to that plugin view
  and clear the query.

Plugin- and inline-view execute paths (`handlePluginExecute`,
`handleInlineExecute`) only handle `"Dismiss"` since `ShowCustomUI`
is not meaningful from within a plugin-owned surface.

### Footer priority

`pluginFooter` (custom view) > `inlineFooter` (when inline slot is
selected) > footer derived from `results[listSelectedIndex].actions`.
Plugins set their footer through the `onFooterChange` action exposed
on `LauncherActions`.

## Result list rendering

`src/launcher/ResultList.tsx` and `ResultRow.tsx`.

`ResultList` is windowed via `useWindowedList`: only `PAGE_SIZE`
rows are mounted at any time, the window shifts via keyboard
selection or mouse-wheel events, and there is no native scroll
container. `ResultRow` renders icon + highlighted title (positions
from `titlePositions`) + optional subtitle (`subtitlePositions`),
both highlighted by `highlightText`.

### Icons (ADR 0027)

Icons travel as a tagged `EntryIcon` union from Rust:
`heroIcon | dataUrl | assetIcon | emoji`. `ResultRow` flattens this
into the prefix-string protocol consumed by the shared `Icon`
component (`src/components/Icon.tsx`):

- `heroicons:<kebab-name>` — resolved by name lookup against
  `@heroicons/react/24/outline` (kebab-to-PascalCase + `Icon`
  suffix). Unknown names fall back to `CommandLineIcon`.
- `emoji:<char>` — rendered as a `<span>`.
- `data:<url>` — rendered as `<img>`.
- `asset:<path>` — filesystem path through Tauri's
  `convertFileSrc`.

The same protocol is used for plugin-registry icons (`icon:` field
on `PluginRegistryEntry`).

## Plugin view registry — named view resolution (ADR 0022)

`src/plugins/registry.ts` is a runtime `Map<pluginId,
PluginRegistryEntry>` populated from two sources:

- **Internal plugins** registered at module load time at the bottom
  of `registry.ts` (`app-launcher`, `system-preferences`,
  `clipboard-manager`).
- **WASM plugins** registered at startup by
  `registerAllWasmPlugins()` (`src/plugins/wasmPluginLoader.ts`)
  after the host queries the backend for loaded manifests.

```ts
interface PluginRegistryEntry {
  label: string;
  description?: string;
  icon?: string;                                          // Icon protocol string
  views?: Record<string, ComponentType<PluginViewProps>>;
  inlineViews?: Record<string, ComponentType<InlineViewProps>>;
  settings?: ComponentType<PluginSettingsProps>;
}
```

`PluginViewRef.view` from the backend is the key into `views` /
`inlineViews`. Lookup helpers `getPluginView(pluginId, viewName)`
and `getPluginInlineView(pluginId, viewName)` return `undefined`
for misses; the containers render `null` when that happens.

### WASM plugin component loading

For each WASM plugin `registerWasmPlugin()` builds dynamic
`import()` factories (wrapped through `launcherComponent` /
`settingsComponent` for `Suspense` + lazy semantics) targeting URLs
under the custom protocol scheme:

```
torchsnap-plugin://localhost/<pluginId>/<bundleFile>
```

Each declared view name maps to an export name on that bundle. The
bundle is fetched only when the view is first mounted. Scoped CSS
declared in the manifest (`launcherCss` / `settingsCss`) is
injected via `injectPluginCss` for the matching webview only.

## Plugin component contract (ADR 0028)

Plugin components receive **only per-render data** through props
(`results`, `data`, `query`, `matchedPrefix`, `selected`).
Everything else flows through React context.

### Provider placement

Two containers in `Launcher.tsx` plus `PluginSectionContent` in
`src/settings/SettingsPanel.tsx` wrap their plugin component mount
in a `<PluginContextProvider>`:

- `PluginViewContainer` — full custom view (`PluginViewProps` + view
  name).
- `InlineViewContainer` — inline view (`InlineViewProps` + view
  name).
- Settings panel — `PluginSettingsProps` (empty struct), no launcher
  slice.

The provider carries three slices:

- `info: { id, enabled }` — plugin id plus reactive `enabled` flag,
  read from setting `enabled.<plugin-id>` via
  `useOptionalPluginEnabled`.
- `runtime: { sendMessage, logger }` — capabilities every plugin
  gets. `sendMessage` is bound to the active plugin's id;
  `logger` is created per active plugin via `createLogger(id)`.
- `launcher: { goBack, dismiss, onExecute, onFooterChange,
  setDisplayQuery, mouseActiveRef }` — only present inside the
  launcher tree. Inline views receive a slice with no-op `goBack` /
  `setDisplayQuery` (with dev-mode warnings) since neither makes
  sense above the result list.

`PluginContextProvider` also drives the legacy `LoggerContext` so
host code reading via `useLogger()` keeps working.

### Hooks

In `src/contexts/`:

- `usePluginInfo()` — info slice. Available everywhere.
- `usePluginRuntime()` — runtime slice. Available everywhere.
- `useLauncher()` — launcher slice. Throws outside the launcher
  tree.
- `usePluginSetting<T>(key)` — reactive accessor scoped to the
  active plugin's namespace. Resolves to setting key
  `plugins.<id>.<key>`. Returns `[value, setValue]`.

Per-render data stays as props because hoisting it would invalidate
the context value on every keystroke.

## Live updates from plugins (ADR 0016)

Plugins push real-time updates to their mounted component through
the same `sendMessage` channel that handles request/response
calls. The frontend signature:

```ts
sendMessage<TPayload, TResult, TStream>(
  method: string,
  payload: TPayload,
  onMessage?: (msg: TStream) => void,
): Promise<TResult>;
```

`sendPluginMessage` (`src/lib/pluginMessage.ts`) wraps the
`plugin_message` Tauri command, allocating a `Channel<TStream>`
and wiring `onMessage`. The plugin's backend `handle_message`
receives that channel, returns an initial payload through the
promise, and stores the channel handle for its background thread
to push subsequent items into. When the component unmounts the
channel reference is dropped — the backend detects the closed
channel on its next send and discards it; no explicit
unsubscribe.

**WASM plugins do not support streaming.** The WASM bridge silently
drops the streaming channel; `onMessage` is part of the public
signature only for symmetry with native plugins. The shim
documents this and points at ADR 0030's future
`messaging-stream` sub-interface.

## TypeScript SDK package — `@torchsnap/plugin-sdk`

`packages/plugin-sdk/` is a private, source-only npm package
consumed via `file:` protocol with a custom exports map (see
`packages/plugin-sdk/package.json`):

| Subpath        | Purpose                                                  |
| -------------- | -------------------------------------------------------- |
| `.`            | Public type exports — `PluginViewProps`, `InlineViewProps`, `PluginSettingsProps`, `SourcedEntry`, `Action`, `EntryIcon`, `FooterState`, `Logger`, etc. (`src/types/`) |
| `/hooks`       | Runtime shim exposing `usePluginInfo`, `usePluginRuntime`, `useLauncher`, `usePluginSetting`, `useWindowedList` |
| `/components`  | Shared UI primitives shim (`Switch`, `Slider`, `Section`, `Entry`, `List`) |
| `/keybindings` | `useKeyBindings`, `LAYER` shim |
| `/utils`       | Utilities shim (`highlightText`) |
| `/testing`     | Test harness (`MockPluginContextProvider`, setup) |
| `/vite`        | Vite plugin/config helpers for plugin builds |
| `/theme.css`   | Shared theme CSS |

The shims do not bundle implementations. Each one declares a slice
on the ambient `TorchsnapGlobal` interface and lazily resolves to
`window.__torchsnap.<slice>` at call time. TypeScript merges the
slice declarations across files, so a plugin's compiled view of
`__torchsnap` is exactly the union of slices its imports pull in.

The host's `initPluginSdk()` (`src/lib/sdk.ts`) populates
`window.__torchsnap` once at startup, before any plugin bundle is
fetched, with the real implementations:

```ts
window.__torchsnap = {
  React, jsxRuntime,
  hooks: { usePluginInfo, usePluginRuntime, useLauncher,
           usePluginSetting, useWindowedList },
  keybindings: { useKeyBindings, LAYER },
  components: { Switch, Slider, Section, Entry, List },
  utils: { highlightText },
};
```

`react` and `react/jsx-runtime` are aliased at build time (Vite) to
the SDK's React shims, so plugin bundles ship import statements
only; the host owns React, the context, the hook implementations
and the shared components. This keeps plugin bundles small and
guarantees a single React instance across host and plugins.
