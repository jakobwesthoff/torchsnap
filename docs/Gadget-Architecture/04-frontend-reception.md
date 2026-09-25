# Frontend Reception and Rendering

How gadget output reaches the launcher webview, gets merged into the
result list, and is rendered as either standard rows, an inline
component above the list, or a full gadget-claimed view.

## Streaming results — `useSearch`

`src/launcher/hooks/useSearch.ts`.

Each query change opens a fresh `Channel<SearchMessage>` (Tauri's
typed channel API) and invokes the `search` command with it. The
backend pushes one `searchResults` message per source (the catalog
layer plus every query gadget) and a final `done` message. The
channel mirrors the `SearchMessage` union from `src/types.ts`, which
itself mirrors `src-tauri/src/commands/types.rs`.

### Per-source accumulator

Entries are kept in a `Map<sourceKey, SourcedEntry[]>` ref, keyed via
`resultSourceKey(message.source)`:

- `catalog` for the aggregated catalog layer (one batch covering all
  catalog-providing gadgets),
- `gadget:<id>` for each query gadget.

Every incoming `searchResults` replaces that source's batch
wholesale. An empty batch evicts the source's entry from the Map.
That is how a gadget transitioning from results to empty (e.g.
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

`customGadgetView`, `inlineGadgetView` and `matchedPrefix` each
arrive on every `searchResults` message. They follow a different
rule than entries:

- The first message of a generation overwrites unconditionally
  (including with `null`) so stale view refs from a previous query
  cannot leak through.
- Subsequent messages of the same generation use "first non-null
  wins" semantics: the catalog batch carries `null` view refs and
  must not clobber a pending inline/custom view from a slower gadget
  message.

Empty queries reset entries and view refs synchronously during
render via the `prevQuery` derived-state pattern, so gadget
components unmount on the same render and their effects cannot
re-inject stale display-query state.

The hook returns `{ results, customGadgetView, inlineGadgetView,
matchedPrefix, loading }`.

## `Launcher` component

`src/launcher/Launcher.tsx` orchestrates input, search, and the five
content states.

### Dual query state

Two strings:

- `displayQuery`: what the input renders.
- `searchQuery`: what `useSearch` consumes.

`setQuery()` writes both. `setDisplayQuery()` (passed into gadgets
via `LauncherActions`) writes only the display, letting a gadget
adjust the visible text without retriggering search. The next user
keystroke resyncs both. `handleGoBack` currently clears both to the
empty string. There is a TODO to snapshot/restore the pre-gadget
query.

### Two custom-view sources

A gadget-claimed view can come from either:

1. **`searchGadgetView`**: the `customGadgetView: GadgetViewRef`
   from `useSearch`, set when an enabled gadget returned `CustomUI`
   for the current query.
2. **`executeGadgetView`**: set locally when:
   - `search_execute` returns `PostAction::ShowCustomUI { view, data
     }` (the entry's `source` becomes the gadget id), or
   - the backend emits the `activate-gadget-custom-ui` Tauri event
     because a global shortcut handler returned `ShowCustomUI`. The
     payload carries `{ gadgetId, view, data }`.

Resolution: `executeGadgetView ?? searchGadgetView`. Both sides are
full `GadgetViewRef` values with an explicit `view` name and
optional opaque `data`, with no implicit `"default"` fallback.

### Inline view

`inlineGadgetView` from `useSearch` is rendered above the result
list when no custom view is active (`customGadgetView == null`).
The inline slot occupies index 0 of the keyboard navigation model,
so the result list's logical index becomes `selectedIndex - 1` and
its total nav count becomes `results.length + 1`. `selected` is
threaded into the inline component as a prop so it can render its
own selected state.

### Five content states

Selected by an if/else chain in `Launcher.tsx`:

1. **Measurement dummy**: initial render reports the card's real
   dimensions to the backend before the first show.
2. **Empty**: no inline view and no results. Only the search input
   renders, with no content section and no footer.
3. **Gadget custom view**: `GadgetViewContainer` mounts the
   registered React component. The standard list is hidden and
   keyboard navigation is disabled (`enabled: customGadgetView ===
   null`).
4. **Inline + list**: `InlineViewContainer` above `ResultList`.
   Inline slot participates in nav at index 0.
5. **List only**: `ResultList` only.

### Action execution and `PostAction`

Each `SourcedEntry` carries its actions as a list of
`{ slot, label }` (`Action` and `ActionSlot` in `src/types.ts`), in
slot order. The commands stay in the host. `src/launcher/actionSlots.ts`
owns the table from slot to key (`SLOT_KEYS`) and to default label:

| Slot | Key | Default label |
|---|---|---|
| `primary` | Enter, click on the row | entry title |
| `secondary` | Cmd+Enter | entry title |
| `copy` | Cmd+C | "Copy" |
| `reveal` | Cmd+Shift+R | "Reveal in Finder" |
| `delete` | Cmd+Backspace | "Delete" |
| `openSettings` | Cmd+, | "Open settings" |

`SLOT_KEYS` writes Cmd as the `Meta` modifier, which matches Cmd on
macOS and Ctrl elsewhere.
`actionLabel` picks the gadget's label, then the slot's default,
then the entry's title. `slotBindings` gives the keyboard navigation
the combos of the selected entry's filled slots other than
`primary`, whose Enter binding the launcher registers once.

List-mode `executeEntry(entry, slot = "primary")` returns early when
the entry has no action in `slot` (`hasSlot`). Otherwise it invokes
`search_execute` with `{ source, entryId, slot }` and matches the
returned `PostAction`:

- `"Dismiss"`: dismiss the launcher.
- `"Nothing"` / `"KeepOpen"`: explicit no-op.
- `{ ShowCustomUI: { view, data } }`: switch to that gadget view
  and clear the query.

A click on a row runs its `primary` action, like Enter.

Gadget- and inline-view execute paths (`handleGadgetExecute`,
`handleInlineExecute`) back `onExecute(entryId, slot)` on
`LauncherActions`. They send the view's gadget id as `source` and
only handle `"Dismiss"` since `ShowCustomUI` is not meaningful from
within a gadget-owned surface.

`openSettings()` on `LauncherActions` (`handleGadgetOpenSettings`,
`handleInlineOpenSettings`) calls `openGadgetSettings` in
`src/launcher/openGadgetSettings.ts` with the view's gadget id. That
invokes the `gadget_open_settings` command, which opens the Settings
window on that gadget's section, and then dismisses the launcher.
When the command fails, the launcher stays open and the promise
rejects.

### Footer priority

`gadgetFooter` (custom view) > `inlineFooter` (when inline slot is
selected) > footer derived by `actionsToFooterState` from
`results[listSelectedIndex].actions`: the `primary` action on Enter,
every other filled slot as a hint. Gadgets set their footer through
the `onFooterChange` action exposed on `LauncherActions`.

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

- `heroicons:<kebab-name>`: resolved by name lookup against
  `@heroicons/react/24/outline` (kebab-to-PascalCase + `Icon`
  suffix). Unknown names fall back to `CommandLineIcon`.
- `emoji:<char>`: rendered as a `<span>`.
- `data:<url>`: rendered as `<img>`.
- `asset:<path>`: filesystem path through Tauri's
  `convertFileSrc`.

The same protocol is used for gadget-registry icons (`icon:` field
on `GadgetRegistryEntry`).

## Gadget view registry — named view resolution (ADR 0022)

`src/gadgets/registry.ts` is a runtime `Map<gadgetId,
GadgetRegistryEntry>` populated from two sources:

- **Internal gadgets** registered at module load time at the bottom
  of `registry.ts` (`app-launcher`, `system-preferences`,
  `clipboard-manager`).
- **WASM gadgets** registered at startup by
  `registerAllWasmGadgets()` (`src/gadgets/wasmGadgetLoader.ts`)
  after the host queries the backend for loaded manifests.

```ts
interface GadgetRegistryEntry {
  label: string;
  description?: string;
  icon?: string;                                          // Icon protocol string
  views?: Record<string, ComponentType<GadgetViewProps>>;
  inlineViews?: Record<string, ComponentType<InlineViewProps>>;
  settings?: ComponentType<GadgetSettingsProps>;
}
```

`GadgetViewRef.view` from the backend is the key into `views` /
`inlineViews`. Lookup helpers `getGadgetView(gadgetId, viewName)`
and `getGadgetInlineView(gadgetId, viewName)` return `undefined`
for misses; the containers render `null` when that happens.

### WASM gadget component loading

For each WASM gadget `registerWasmGadget()` builds dynamic
`import()` factories (wrapped through `launcherComponent` /
`settingsComponent` for `Suspense` + lazy semantics) targeting URLs
under the custom protocol scheme:

```
torchsnap-gadget://localhost/<gadgetId>/<bundleFile>
```

Each declared view name maps to an export name on that bundle. The
bundle is fetched only when the view is first mounted. Scoped CSS
declared in the manifest (`launcherCss` / `settingsCss`) is
injected via `injectGadgetCss` for the matching webview only.

## Gadget component contract (ADR 0028)

Gadget components receive **only per-render data** through props
(`results`, `data`, `query`, `matchedPrefix`, `selected`).
Everything else flows through React context.

### Provider placement

Two containers in `Launcher.tsx` plus `GadgetSectionContent` in
`src/settings/SettingsPanel.tsx` wrap their gadget component mount
in a `<GadgetContextProvider>`:

- `GadgetViewContainer`: full custom view (`GadgetViewProps` + view
  name).
- `InlineViewContainer`: inline view (`InlineViewProps` + view
  name).
- Settings panel: `GadgetSettingsProps` (empty struct), no launcher
  slice.

The provider carries three slices:

- `info: { id, enabled }`: gadget id plus reactive `enabled` flag,
  read from setting `enabled.<gadget-id>` via
  `useOptionalGadgetEnabled`.
- `runtime: { sendMessage, logger }`: capabilities every gadget
  gets. `sendMessage` is bound to the active gadget's id;
  `logger` is created per active gadget via `createLogger(id)`.
- `launcher: { goBack, dismiss, openSettings, onExecute,
  onFooterChange, setDisplayQuery, mouseActiveRef }`: only present
  inside the launcher tree. Inline views receive a slice with no-op `goBack` /
  `setDisplayQuery` (with dev-mode warnings) since neither makes
  sense above the result list.

`GadgetContextProvider` also drives the legacy `LoggerContext` so
host code reading via `useLogger()` keeps working.

### Hooks

In `src/contexts/`:

- `useGadgetInfo()`: info slice. Available everywhere.
- `useGadgetRuntime()`: runtime slice. Available everywhere.
- `useLauncher()`: launcher slice. Throws outside the launcher
  tree.
- `useGadgetSetting<T>(key)`: reactive accessor scoped to the
  active gadget's namespace. Resolves to setting key
  `gadgets.<id>.<key>`. Returns `[value, setValue]`.

Per-render data stays as props because hoisting it would invalidate
the context value on every keystroke.

## Live updates from gadgets (ADR 0016)

Gadgets push real-time updates to their mounted component through
the `sendMessage` function from `useGadgetRuntime()`. The full
messaging contract (request/response flow, backend routing, WASM
limitations, `useGadgetStream` hook) is documented in
[05-gadget-messaging.md](05-gadget-messaging.md). This section
covers only the frontend lifecycle aspects.

When a component unmounts, the Tauri channel reference is dropped.
The backend detects the closed channel on its next send and discards
it, so there is no explicit unsubscribe call. `useGadgetStream`
(`src/hooks/useGadgetStream.ts`) wraps this pattern and re-issues
the command when `method` or `payload` identity changes.

## TypeScript SDK package — `@torchsnap/gadget-sdk`

`packages/gadget-sdk/` is a private, source-only npm package
consumed via `file:` protocol with a custom exports map (see
`packages/gadget-sdk/package.json`):

| Subpath        | Purpose                                                  |
| -------------- | -------------------------------------------------------- |
| `.`            | Public type exports: `GadgetViewProps`, `InlineViewProps`, `GadgetSettingsProps`, `SourcedEntry`, `Action`, `ActionSlot`, `EntryIcon`, `FooterState`, `Logger`, etc. (`src/types/`) |
| `/hooks`       | Runtime shim exposing `useGadgetInfo`, `useGadgetRuntime`, `useLauncher`, `useGadgetSetting`, `useWindowedList` |
| `/components`  | Shared UI primitives shim (`Switch`, `Slider`, `Section`, `Entry`, `List`) |
| `/keybindings` | `useKeyBindings`, `LAYER` shim |
| `/utils`       | Utilities shim (`highlightText`) |
| `/testing`     | Test harness (`MockGadgetContextProvider`, setup) |
| `/vite`        | Vite plugin/config helpers for gadget builds |
| `/theme.css`   | Shared theme CSS |

The shims do not bundle implementations. Each one declares a slice
on the ambient `TorchsnapGlobal` interface and lazily resolves to
`window.__torchsnap.<slice>` at call time. TypeScript merges the
slice declarations across files, so a gadget's compiled view of
`__torchsnap` is exactly the union of slices its imports pull in.

The host's `initGadgetSdk()` (`src/lib/sdk.ts`) populates
`window.__torchsnap` once at startup, before any gadget bundle is
fetched, with the real implementations:

```ts
window.__torchsnap = {
  React, jsxRuntime,
  hooks: { useGadgetInfo, useGadgetRuntime, useLauncher,
           useGadgetSetting, useWindowedList },
  keybindings: { useKeyBindings, LAYER },
  components: { Switch, Slider, Section, Entry, List },
  utils: { highlightText },
};
```

`react` and `react/jsx-runtime` are aliased at build time (Vite) to
the SDK's React shims, so gadget bundles ship import statements
only; the host owns React, the context, the hook implementations
and the shared components. This keeps gadget bundles small and
guarantees a single React instance across host and gadgets.
