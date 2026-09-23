# Missing or misnamed gadget bundle export suspends the view forever ("Loading…" with no error)

**Kind:** possible-bug
**Severity:** medium
**Area:** src/gadgets/wasmPluginLoader.ts, src/lib/gadgetComponent.tsx

## Problem

WASM gadget views are created by resolving a named export from the
dynamically imported bundle
(`src/gadgets/wasmPluginLoader.ts:62-68`):

```ts
views[viewName] = launcherComponent(() =>
  import(/* @vite-ignore */ bundleUrl).then((mod) => ({
    default: mod[exportName],
  })),
);
```

If `exportName` (taken from the gadget's `manifest.toml`
`[frontend]` tables) does not exist on the module, `mod[exportName]`
is `undefined` and the factory *resolves successfully* with
`{ default: undefined }`.

The Suspense wrapper in `gadgetComponent`
(`src/lib/gadgetComponent.tsx:44-66`) then never leaves the pending
state:

```ts
promise = factory().then(
  (mod) => { Component = mod.default; },   // Component = undefined
  (err) => { error = err; },
);
...
const Wrapper = (props: P) => {
  if (error) throw error;
  if (Component) return <Component {...props} />;  // stays false
  throw load();                                     // resolved promise
};
```

`error` stays `null`, `Component` stays `undefined`, and every
render throws the already-resolved promise — React retries, hits
the same state, and the `<Suspense>` fallback ("Loading…",
`src/launcher/Launcher.tsx:654`) is displayed indefinitely. Nothing
is logged; the devtools console shows no failure.

The same pattern applies to inline views
(`wasmPluginLoader.ts:75-84`) and custom settings components
(`wasmPluginLoader.ts:89-97`).

A manifest typo (component renamed in the bundle but not in
`[frontend.settings] component = "..."`, or a stale `views` map) is
exactly the kind of mistake a gadget author will make during
development, and the current failure mode gives no diagnostic.

## Impact

Gadget view/settings panels hang on the loading fallback forever
with zero feedback; authors have to guess that the export name is
wrong.

## Suggested fix

Reject inside the factory when the export is missing:

```ts
import(/* @vite-ignore */ bundleUrl).then((mod) => {
  const component = mod[exportName];
  if (component == null) {
    throw new Error(
      `gadget ${gadgetId}: bundle ${bundleUrl} has no export "${exportName}"`,
    );
  }
  return { default: component };
})
```

That routes the case into `gadgetComponent`'s existing `error`
path, which throws to the nearest error boundary (verify one exists
above the Suspense boundary in the launcher; add one if not, since
a throwing gadget view must not take down the whole launcher).
