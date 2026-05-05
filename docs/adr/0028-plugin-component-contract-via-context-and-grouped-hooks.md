# 28. Plugin component contract via context and grouped hooks

Date: 2026-04-08

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

Plugin React components — `PluginViewProps`, `InlineViewProps`, and
`PluginSettingsProps` consumers — historically received every ambient
capability as a flat prop:

* `pluginId`
* `usePluginSetting` (a hook handed down as a prop, factory-bound to the
  plugin's namespace)
* `sendMessage`
* `logger`
* `goBack`, `dismiss`, `onExecute`, `onFooterChange`, `setDisplayQuery`,
  `mouseActiveRef`

Several issues followed from this design:

1. **`usePluginSetting` is a hook passed as a prop.** Calling a hook held
   in a prop is unusual React. The Rules of Hooks still apply, but the
   type system can't enforce that the prop is invoked in a stable order
   from a component top — easy to get subtly wrong, hard to lint for.
1. **Sub-components need prop threading.** Extracting any helper from a
   settings panel forces every nested component to re-receive `pluginId`,
   `usePluginSetting`, the (would-be) `pluginEnabled` flag, and so on
   through every level. The composability tax compounds with depth.
1. **Adding a new ambient capability is a breaking change.** Anything new
   forces every plugin to update its destructure list and the props
   interface bumps for everyone simultaneously.
1. **Symptom: the `pluginEnabled` prop addition.** Calculator's settings
   panel currently does `useSetting<bool>("enabled.calculator")` directly
   because it has no clean way to read its own enabled state. Patching
   that with another prop would have made the contract worse, not better.

The migration plan calls for the calculator plugin to move from a native
Rust plugin to a WASM plugin. That migration is a forcing function — its
React components will be authored against whatever the new contract is,
and we want them born into a clean shape rather than refactored twice.

## Decision

Replace the props-based contract with a single React context, exposed
through three logically-grouped hooks plus one parametric setting hook:

````ts
usePluginInfo():    { id: string; enabled: boolean }
usePluginRuntime(): { sendMessage: PluginSendMessage; logger: Logger }
useLauncher():      { goBack, dismiss, onExecute, onFooterChange,
                      setDisplayQuery, mouseActiveRef }
usePluginSetting<T>(key): [value: T, setValue: (v: T) => Promise<void>]
````

The implementation lives in `src/contexts/`:

* `PluginContext.tsx` — the `createContext` declaration plus value-shape
  types (`PluginInfo`, `PluginRuntime`, `LauncherActions`,
  `PluginContextValue`, `PluginSendMessage`).
* `PluginContextProvider.tsx` — the provider component. Memoizes the
  context value and also drives the legacy `LoggerContext` so any code
  reading via `useLogger()` continues to work without changes.
* `usePluginInfo.ts`, `usePluginRuntime.ts`, `useLauncher.ts`,
  `usePluginSetting.ts` — the four hooks.
* `index.ts` — barrel export for host imports.

The host wraps every plugin component mount in `<PluginContextProvider>`:

* `src/launcher/Launcher.tsx` — `PluginViewContainer` and
  `InlineViewContainer` build the `info` / `runtime` / `launcher` slices
  from existing host state and pass them to the provider before
  rendering the plugin's view component.
* `src/settings/SettingsPanel.tsx` — `PluginSectionContent` does the
  same with the `info` and `runtime` slices only (settings panels have
  no `launcher` slice).

`useLauncher()` throws a clear error if called from a settings context
(where `launcher` is `undefined` on the context value). Fail loud, not
silent.

### Per-render data stays as props

`results`, `data`, `query`, `matchedPrefix`, and `selected` remain on
`PluginViewProps` / `InlineViewProps`. They genuinely change every
keystroke and would invalidate the context value identity on every
render if hoisted, forcing every consumer to re-render.

The post-refactor prop interfaces (mirrored in both
`src/plugins/types.ts` and `packages/plugin-sdk/src/types/plugin.ts`):

````ts
interface PluginViewProps {
  results: SourcedEntry[];
  data?: unknown;
  query: string;
  matchedPrefix: string;
}

interface InlineViewProps {
  data: unknown;
  query: string;
  matchedPrefix: string;
  selected: boolean;
}

interface PluginSettingsProps {} // collapsed to empty
````

`PluginSettingsProps` collapsing to empty is the design's strongest
signal that the refactor is correct: settings panels were 100% ambient
capability with zero per-render data.

### SDK exposure

WASM plugins live in their own bundles outside the host's React tree.
They reach the hooks via the existing `window.__torchsnap` shim
mechanism:

* `src/lib/sdk.ts` populates `window.__torchsnap.hooks` with the four
  host implementations during `initPluginSdk()`.
* `packages/plugin-sdk/src/shims/hooks.ts` re-exports them under the
  `@torchsnap/plugin-sdk/hooks` subpath so plugin code does
  `import { usePluginInfo } from "@torchsnap/plugin-sdk/hooks"`.

This is the same bridge pattern already in use for `react` and
`react/jsx-runtime` (see `packages/plugin-sdk/src/shims/react.ts`).

### Single context, optional launcher slice

Implementation under the hood is a single `PluginContext` whose value
carries an optional `launcher` field. Settings mounts pass `info + runtime`; launcher view/inline mounts pass `info + runtime + launcher`.
The three hooks read from the same context and return the appropriate
slice.

A single shared context (rather than two or three split internal
contexts) was chosen because:

* Plugin components typically read multiple slices anyway, so multiple
  contexts would just multiply provider overhead without reducing
  re-renders.
* One context value memo is easier to reason about than coordinating
  multiple memo'd providers.

### Big-bang migration

All native plugins (`calculator`, `clipboard`, `emoji`, plus the
parameterless `bangs` settings) and both template plugins
(`plugins/template/`, `plugins/calculator/`) migrate in the same change
set as the context infrastructure. No transitional dual-API period.

## Alternatives considered

* **Per-capability hooks** (`useSendMessage()`, `useLogger()`,
  `usePluginId()`, `useGoBack()`, …). Rejected as too noisy — every
  plugin component would import a dozen hooks.
* **One mega-hook** (`usePluginContext()` returning everything as a flat
  object). Rejected as too coarse — exposes the launcher slice in
  settings contexts where it shouldn't be available, and provides no
  way to fail fast on misuse.
* **Multiple internal React contexts** (`PluginInfoContext`,
  `PluginRuntimeContext`, `LauncherContext`). Rejected for the
  reasoning above — single context is simpler and the consumer-visible
  API stays identical.
* **Moving per-render data into context.** Rejected — every keystroke
  would invalidate the context value, forcing every consumer to
  re-render even if they only read identity or runtime capabilities.
* **Transitional dual-API.** Rejected as over-engineering — the
  migration is a single PR-sized change and there are no third-party
  plugins to ease yet.

## Consequences

* Plugin components are dramatically simpler: settings panels with
  helper sub-components no longer thread props through every level. Any
  child component can call `usePluginSetting("foo")` directly.
* Adding a new ambient capability (e.g., a future `usePluginPermissions`)
  is purely additive — no existing component changes.
* The `pluginEnabled` follow-on prop that the original migration plan
  contemplated never lands; calculator's settings panel reads its
  enabled flag via `usePluginInfo().enabled` instead.
* Plugin authors learn one mental model — context + four hooks — for
  every component kind.
* The legacy `LoggerContext` / `useLogger()` path stays intact. The
  PluginContextProvider also drives it, so host components that aren't
  plugin-component leaves can continue reading the logger via the older
  context unchanged.
* WASM plugins consume the same hooks via
  `@torchsnap/plugin-sdk/hooks`, which resolves to a thin shim that
  reads `window.__torchsnap.hooks` at call time. Same bridge model as
  `react` and `react/jsx-runtime`.