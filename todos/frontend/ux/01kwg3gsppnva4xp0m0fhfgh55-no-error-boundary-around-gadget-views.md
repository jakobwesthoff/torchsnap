---
kind: bug
severity: high
status: open
area: [src/launcher/Launcher.tsx]
---

# No React error boundary anywhere — a throwing gadget view blanks the entire launcher

The launcher, settings, and devtools windows are all affected.

## Problem

`grep -rn "ErrorBoundary\|componentDidCatch\|getDerivedStateFromError" src/`
returns nothing: none of the three webview React trees (launcher,
settings, devtools) contain an error boundary.

Gadget custom-UI and inline-view components are dynamically loaded
third-party code (WASM gadget frontend bundles, loaded via
`torchsnap-gadget://` in `src/gadgets/wasmPluginLoader.ts`) and are
rendered directly into the launcher tree with only a `<Suspense>`
wrapper (`src/launcher/Launcher.tsx:653-667`):

```tsx
<Suspense fallback={...}>
  <PluginViewContainer ... />
</Suspense>
```

`Suspense` catches thrown *promises*, not thrown *errors*. Per
React semantics, an error thrown during render with no error
boundary above it unmounts the whole component tree at the root.

So any render-time exception in any gadget's view component — a
`TypeError` on unexpected `data`, a bug in a third-party gadget's
settings panel, or the poisoned-loader case in
`01kwg3gsppnva4xp0m0fhfgh54-missing-gadget-export-suspends-forever.md`
once it starts throwing — leaves the launcher (or settings window)
as a blank white surface until the webview is reloaded. The
launcher window is long-lived and hidden/shown rather than
recreated, so the blank state persists across launcher
invocations; the user's only recovery is restarting the app.

## Impact

A single misbehaving gadget frontend takes the entire launcher
down, persistently. This inverts the isolation story: the WASM
side sandboxes gadget *backends* carefully, while a gadget
*frontend* can trivially disable the app's primary UI.

## Suggested fix

Add an error boundary component and wrap each gadget mount point:

- around `PluginViewContainer` and `InlineViewContainer` in
  `src/launcher/Launcher.tsx` (fallback: an inline error card with
  the gadget id and a "go back" affordance calling the existing
  `handleGoBack`),
- around the gadget settings component mount in the settings
  window,
- optionally one at each window root as a last-resort "reload this
  window" screen.

Log caught errors through `createLogger(gadgetId)` so gadget
frontend crashes appear in devtools attributed to the gadget.
