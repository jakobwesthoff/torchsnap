# 16. Allow plugins to stream live updates to their frontend component

Date: 2026-03-27

## Status

Accepted

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

## Context

Some plugins produce data asynchronously after their custom UI is
already mounted — clipboard history entries arriving in real time, file
watcher events, or other background-produced data. The current
`search()` → results-as-props flow is pull-based and only triggers on
query changes. There is no mechanism for a plugin's backend to push
updates to its frontend component while the UI is open.

ADR 0013 (topic 4) already defines `handle_message` with a
`Channel<StreamItem>` parameter and a corresponding frontend
`sendMessage` with an `onMessage` callback. The channel infrastructure
exists in the design but is not yet implemented. This ADR decides how
plugins use that infrastructure to establish live update streams.

## Decision

A plugin that needs to push live updates to its frontend component uses
the message bus from ADR 0013 as follows:

1. **Frontend:** On mount, the custom UI component calls `sendMessage`
   with a subscription method (e.g., `"subscribe_updates"`) and an
   `onMessage` callback. This opens a Tauri channel scoped to the
   invocation.

1. **Backend:** The plugin's `handle_message` implementation receives
   the channel, stores it, and returns an initial payload (e.g., the
   current clipboard history). The stored channel is shared with the
   plugin's background thread (e.g., via `Arc<Mutex<...>>`).

1. **Background thread:** When new data arrives (e.g., a new clipboard
   entry), the thread sends it through the stored channel. The frontend
   receives it via the `onMessage` callback.

1. **Cleanup:** When the frontend component unmounts, it drops its end
   of the channel. The backend detects the closed channel on the next
   send attempt and removes it. No explicit unsubscribe message is
   needed.

On the frontend, a reusable React hook wraps this pattern so individual
plugin components don't reimplement the subscribe/accumulate/cleanup
logic. The hook calls `sendMessage` on mount, feeds streamed items into
reactive state, and tears down on unmount.

This approach avoids Tauri's global event bus entirely. All
communication is scoped to the plugin boundary through the host-mediated
message bus, consistent with the WASM-friendly dispatch design from
ADR 0013.

## Consequences

* Plugins can push real-time updates to their UI without polling or
  global events.
* The channel lifecycle is tied to the component mount/unmount cycle —
  no leaked subscriptions.
* Requires the `handle_message` + `Channel<StreamItem>` implementation
  from ADR 0013 topic 4 to be wired up (currently a TODO).
* The reusable subscription hook reduces boilerplate for future
  streaming plugins.
* Backend plugins that hold a channel reference must handle the channel
  being closed at any time (component unmounted, user navigated away).