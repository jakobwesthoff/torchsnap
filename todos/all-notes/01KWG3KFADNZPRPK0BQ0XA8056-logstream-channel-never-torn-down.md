---
kind: bug
severity: medium
status: open
area: [src/devtools/console/useLogStream.ts]
---

# Devtools log subscription is never torn down — leaked backend stream per subscribe

## Problem
The `useSyncExternalStore` subscribe callback in `useLogStream`
(`src/devtools/console/useLogStream.ts:156-202`) creates a Tauri
`Channel` and invokes `devtools_log_subscribe`, but its cleanup only
flips a local flag:

```ts
return () => {
  cancelled = true;
  unsubscribe();
};
```

Nothing closes the channel or informs the backend. On the Rust side,
`devtools_log_subscribe` spawns a task that loops forever until
`channel.send(...)` fails
(`src-tauri/src/wasm/logging/commands.rs:97-139`) — and sends only
fail once the *webview* is destroyed. The JS `Channel` likewise only
unregisters its raw callback when the backend sends an end message
(`@tauri-apps/api` `core.js`, `cleanupCallback` called on `'end' in
rawMessage`), which the streaming task never does while the webview
lives.

So every invocation of the subscribe callback permanently leaks, for
the lifetime of the devtools window:
- one backend tokio task holding a broadcast receiver,
- one registered JS callback receiving and deserializing every log
  batch, guarded into a no-op by `cancelled`.

This fires in practice: `src/devtools/main.tsx:17` renders the app in
`<StrictMode>`, so in dev builds React subscribes, unsubscribes, and
resubscribes once on mount — leaving one dead subscription streaming
duplicate IPC batches next to the live one from the moment the window
opens. In production the leak occurs on every `ConsoleTab`
unmount/remount; `DevToolsPanel.tsx:32` conditionally renders the tab
(`{activeTab === "console" && <ConsoleTab />}`), so each future tab
switch away and back adds one more permanently-streaming zombie
subscription (unbounded growth with use).

## Impact
Duplicate IPC traffic and serialization work for every log batch,
multiplied by the number of leaked subscriptions; broadcast receivers
that lag also inflate the shared `dropped` accounting. Today the cost
is one extra stream in dev; it becomes unbounded as soon as a second
devtools tab exists or the console is remounted for any other reason.

## Suggested fix
Give the subscription an explicit teardown: e.g. have
`devtools_log_subscribe` accept an id and add a
`devtools_log_unsubscribe(id)` command (or have the backend task
select on a cancellation token passed via a `Drop`-signaling
mechanism), and call it from the cleanup function. Alternatively the
backend task could exit when a per-window "generation" counter
advances (new subscribe from the same window label supersedes the old
task).
