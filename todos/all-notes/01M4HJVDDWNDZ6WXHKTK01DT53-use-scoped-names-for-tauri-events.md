---
title: "Use scoped names for Tauri events"
kind: refactor
component: backend
tags: [api-design]
---
# Use scoped names for Tauri events

Tauri events emitted from Rust to the frontend use flat, unscoped names (`settings-changed`, `activate-gadget-custom-ui`, `react-ready`). As the app grows, this is hard to navigate and creates collision risk. The command side of this naming problem was already renamed in the control channel PR; moving commands into inline gadgets is a separate, independent todo. This todo covers only the events.

## Problem

Event names have no scope, so nothing shows which part of the app an event belongs to, and two unrelated events can end up with the same name.

## Proposed convention

Event names are arbitrary strings with no Rust identifier constraint, so events should use `:` as a separator, for clarity and visual distinction from commands:

```
<namespace>:<event_name>
```

### Events (Rust to JS via `emit()`)

Current names and proposed names:

- `settings-changed` becomes `settings:changed`
- `activate-gadget-custom-ui` becomes `gadget:activate-custom-ui`
- `react-ready` becomes `lifecycle:react-ready`

The `:` separator is preferred over `_` because:

- It visually distinguishes events from Rust identifiers and command names.
- It matches common event naming conventions in other systems (DOM events, Node.js EventEmitter patterns such as `server:listening`).
- Events are not constrained by the `#[tauri::command]` macro, so there is no technical reason to limit them to `_`.

## Technical notes

- Event names are plain strings, so any separator works.
- No behavior change, pure rename.

## Scope

- All `app.emit()` and `app.emit_to()` calls in Rust.
- All `listen()` and `appWindow.listen()` calls in TypeScript.
