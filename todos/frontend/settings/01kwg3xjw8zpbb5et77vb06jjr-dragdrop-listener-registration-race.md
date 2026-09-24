---
kind: bug
severity: medium
status: open
area: [src/settings/sections/GadgetsManagementPanel.tsx]
---

# Gadget install drag-drop listener leaks on fast unmount and double-installs in dev

## Problem
The drag-drop effect registers a Tauri webview listener via a promise
but its cleanup only calls the unlistener if the promise has already
resolved (`src/settings/sections/GadgetsManagementPanel.tsx:119-155`):

```ts
useEffect(() => {
  let unlisten: (() => void) | undefined;
  getCurrentWebview()
    .onDragDropEvent((event) => { ... })
    .then((u) => {
      unlisten = u;
    });
  return () => {
    unlisten?.();
  };
}, []);
```

If the component unmounts before `onDragDropEvent` resolves, cleanup
runs while `unlisten` is still `undefined`; when the promise resolves
afterwards, the listener is registered and never removed.

This is not just theoretical:

- The settings window renders under `<StrictMode>`
  (`src/settings/main.tsx:30`), so in dev builds every mount of the
  Gadgets section runs effect → cleanup → effect synchronously. The
  first registration's promise always resolves *after* its cleanup
  ran, leaving one leaked listener per visit to the section.
- In production, `SectionContent` unmounts the panel on every
  sidebar switch (`SettingsPanel.tsx:109-111`), so a quick
  switch-away recreates the same race.

A leaked listener is not inert: its `drop` branch calls
`runInstall(path, setBanner)` (`GadgetsManagementPanel.tsx:142-146`),
which invokes the `install_gadget_archive` backend command. With one
live and one leaked listener, a single dropped archive is installed
twice — the second attempt fails (already installed) and its error
banner overwrites the success banner.

## Impact
Dev: every drop on the Gadgets section triggers duplicate install
commands and a spurious "Failed to install" banner after each visit
to the section. Prod: leaked listeners accumulate with fast section
switches, each duplicating install work and firing `setState` on
unmounted components.

## Suggested fix
Standard cancellation pattern:

```ts
let cancelled = false;
let unlisten: (() => void) | undefined;
getCurrentWebview().onDragDropEvent(...).then((u) => {
  if (cancelled) u();
  else unlisten = u;
});
return () => {
  cancelled = true;
  unlisten?.();
};
```
