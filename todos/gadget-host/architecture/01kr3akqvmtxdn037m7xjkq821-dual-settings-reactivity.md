# Two parallel reactive-settings systems with different semantics

## Problem

Settings changes flow through two separate reactive pathways, both
fed by the same `settings-changed` Tauri event listener in `lib.rs`:

1. **`SettingsNotifier` + `SettingsWatch<T>`** — tokio `watch`
   channels for non-gadget subsystems (FrecencyStore,
   ControlSocket, WebsiteMetadataService). Coalescing is inherent
   in `watch` semantics (latest value wins).

2. **`CoalescingDispatcher`** — per-gadget mutex-based dispatcher
   for gadget `setting_changed()` callbacks. Has its own dedup
   logic (same key = latest value; different keys = preserve
   chronological ordering).

The event listener in `lib.rs` manually routes to both:

```rust
notifier.notify(&payload.key, value.clone());   // path 1
host.handle_setting_changed(...);               // path 2
```

## Why this is bad

- Two systems solving the same problem (reactive propagation of
  settings changes with coalescing) using incompatible abstractions.
- The routing logic lives outside both systems in `lib.rs`, growing
  a new branch for each new consumer type.
- A new subsystem author must choose between two systems with
  different trade-offs and no clear guidance.
- `CoalescingDispatcher` exists partly because `SettingsNotifier`
  wasn't designed for the gadget lifecycle's synchronous
  `setting_changed()` callback — a design gap, not a fundamental
  constraint.

## Target shape

Unify on a single settings-change bus. One approach: extend
`SettingsNotifier` with a synchronous subscriber variant that handles
the coalescing semantics `CoalescingDispatcher` provides. Gadgets
subscribe the same way as FrecencyStore. The routing logic in
`lib.rs` reduces to `notifier.notify()`. `CoalescingDispatcher`
becomes unnecessary.

## Affected files

- `src-tauri/src/settings/notifier.rs`
- `src-tauri/src/settings/coalescing_dispatcher.rs`
- `src-tauri/src/gadget_host.rs` (settings dispatch path)
- `src-tauri/src/lib.rs` (event listener routing)
