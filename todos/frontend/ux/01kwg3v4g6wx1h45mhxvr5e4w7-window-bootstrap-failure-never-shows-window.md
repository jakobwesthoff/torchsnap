# Settings/devtools bootstrap failure leaves the window invisible with no error

**Kind:** bug
**Severity:** medium

**Area:** src/settings/main.tsx

## Problem
Auxiliary windows are created hidden and only presented when the
frontend emits `react-ready`
(`src-tauri/src/lib.rs:212-218`: `.visible(false)`;
`lib.rs:253`: `win.once("react-ready", ...)`).

The settings bootstrap (`src/settings/main.tsx:20-47`) awaits several
fallible steps before that emit, with no error handling anywhere:

```ts
async function main() {
  await initStore();
  initGadgetSdk();

  const wasmGadgets = await command("wasm_gadgets");
  registerAllWasmGadgets(wasmGadgets, "settings");
  ...
  await getCurrentWebviewWindow().emit("react-ready");
}

main();
```

`main()` is called without `.catch()`. If `initStore()` or the
`wasm_gadgets` IPC command rejects, the promise rejection is
unhandled, nothing renders, and `react-ready` is never emitted. The
devtools window has the same shape
(`src/devtools/main.tsx:13-29`: `await initStore();` → render →
emit, `main();` without catch).

Follow-up behavior on the Rust side makes it worse in a confusing
way: the first "open settings" click builds the hidden window that
then never presents; a *second* click finds the existing window
(`lib.rs:207-210`) and presents it — now visible but blank or
half-initialized, since the failed `main()` never rendered.

## Impact
Any failure in store initialization or gadget listing (backend error,
IPC failure during startup races) turns "open settings"/"open
devtools" into a silent no-op on first use and a blank window on
retry. No error is shown to the user and nothing is logged beyond
the webview console of an invisible window.

## Suggested fix
Wrap the bootstrap in try/catch: on failure, still render a minimal
error surface (or at least `console.error` + render fallback) and
emit `react-ready` so the window presents and the failure is
visible. Alternatively/additionally, the Rust side could present the
window after a timeout as a safety net.
