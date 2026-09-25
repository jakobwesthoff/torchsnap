# 13. Allow plugins to provide custom UI components for the result area

Date: 2026-03-26

## Status

In Progress

Amended by [21. Extend SearchResponse with inline UI and structured variants](0021-extend-search-response-with-inline-ui-and-structured-variants.md)

Amended by [22. Use named view resolution for plugin UI components](0022-use-named-view-resolution-for-plugin-ui-components.md)

Amended by [42. Rename plugins to gadgets](0042-rename-plugins-to-gadgets.md)

Amended by [55. Dispatch entry actions through gadget commands in fixed slots](0055-dispatch-entry-actions-through-gadget-commands-in-fixed-slots.md)

## Context

The standard result list (vertical rows of icon + title + subtitle) works
for most plugins, but some benefit from fundamentally different layouts.
The emoji picker, for example, would show far more results in a grid than
in a list. A calculator plugin might want an inline expression/result
display. A color picker could use swatches.

Rather than building a fixed set of display modes (list, grid, inline)
into the host, we want plugins to be able to provide their own React
components that render into the launcher. This gives plugins full control
over their presentation while keeping the launcher shell (search bar,
card chrome, footer) consistent.

The emoji picker grid is the first consumer and test case for this
system.

## Design Topics

The following topics need decisions. Each is documented as it is
resolved.

### 1. Activation & Lifecycle

*How and when does a plugin's custom UI activate/deactivate?*

Status: **decided**

**Activation** happens through two paths, both producing the same
frontend effect (mount the plugin's component):

- **Search path**: The plugin's `search()` returns a `SearchResponse`
  enum — either `Results(entries)` for standard list rendering, or
  `CustomUI(entries)` to request custom UI. The decision is per-query,
  not per-plugin — a plugin could return standard results for some
  queries and custom UI for others. When `CustomUI` is returned and
  the plugin has exclusive prefix ownership, the backend includes the
  plugin's ID in the search message. The frontend mounts the plugin's
  component in place of `ResultList`.
- **Execute path** (future): `execute()` returns a new `PostAction`
  variant (e.g., `ShowCustomUI`) that tells the frontend to mount the
  plugin's component. This enables "drill-in" flows where a standard
  list entry opens a custom view.

**Deactivation** is implicit — when the signal stops (next search has
no custom-UI plugin, or the user clears the prefix), the frontend
unmounts the component and shows `ResultList` again.

**Mount/unmount, not show/hide.** The component is fully unmounted on
deactivation. This avoids stale state and is acceptable for short-lived
interactions. Scroll position and internal state are lost, which is fine
for the current use cases.

**Deactivation for execute-triggered UI** uses a navigation stack. When
`execute()` returns `ShowCustomUI`, the host snapshots its current state
(query, results, selected index) and mounts the plugin component. The
host provides a `goBack()` callback (see topic 3). For prefix-triggered
UI, no snapshot is needed — editing the query naturally deactivates
through the normal search flow.

**Escape handling** follows the layered keybinding model. The plugin's
UI registers its own Escape handler at a higher layer. If the plugin has
internal state to unwind (sub-menu, selection), it handles Escape and
stops propagation. If there's nothing to unwind, it calls `goBack()`.
The host never needs to know about the plugin's internal state — the
plugin decides when Escape means "go back to the host."

### 2. Host Surface Contract

*What area of the launcher does the plugin take over?*

Status: **decided**

The plugin can only take over the result list area. The search bar, the
launcher card chrome, and the footer remain under the host's control.
The plugin component renders into the slot where `ResultList` normally
appears.

### 3. Plugin Component Interface

*What props/context does the host inject? What does the host expect
back?*

Status: **decided**

The host provides the following props to the plugin component:

**Inputs (host → plugin):**

- `results: ScoredEntry[]` — search results from the normal
  `search()` flow, always provided. The plugin decides whether to
  use them or ignore them.
- `query: string` — current query, stripped of the matched prefix.
- `matchedPrefix: string` — which prefix activated the plugin.
- `goBack(): void` — pop back to previous state (see topic 1).
- `dismiss(): void` — close the launcher.
- `mouseActiveRef: React.RefObject<boolean>` — shared mouse-active
  tracking to suppress hover-selection during keyboard navigation.

**Outputs (plugin → host):**

- `onExecute(entryId: string, actionId: ActionId): void` — plugin
  delegates execution to host. The host owns PostAction handling
  and dismiss logic.
- `onFooterChange(state: FooterState): void` — plugin sets the
  footer content (see topic 7). The plugin is responsible for
  keeping this in sync with its actual keybindings.

For the execute-triggered activation path (future), `results` is an
empty array, `query` and `matchedPrefix` are empty strings. The plugin
relies on its own backend communication.

### 4. Data Flow (Frontend ↔ Backend)

*How does the plugin's React component communicate with its Rust
backend?*

Status: **decided**

Two data paths, used as needed:

1. **Host-mediated (standard):** The host calls the plugin's
   `search()` on the Rust side and passes the results as a prop. The
   plugin component renders them. Execution goes through the
   host-provided `onExecute` callback. This covers simple cases like
   the emoji grid.

2. **Direct (custom):** The plugin communicates with its backend
   through a host-provided `sendMessage` function. This function is
   passed as a prop to the plugin component and is pre-bound to the
   plugin's ID — the component never specifies its own identity.

   On the Rust side, the plugin trait gets a `handle_message` method:

   ```rust
   fn handle_message(
       &self,
       method: &str,
       payload: MessagePayload,
       channel: Channel<StreamItem>,
   ) -> Result<MessageResponse>;
   ```

   `MessagePayload`, `MessageResponse`, and `StreamItem` are newtypes
   over `serde_json::Value`. The newtypes prevent accidental swapping
   of return values and stream items at the trait boundary. Type safety
   within a plugin is the plugin's responsibility — it serializes and
   deserializes its own concrete types on both sides (Rust and TS).

   On the frontend, the prop signature is:

   ```typescript
   sendMessage<TResult, TStream = never>(
       method: string,
       payload: unknown,
       onMessage?: (msg: TStream) => void,
   ): Promise<TResult>
   ```

   Without `onMessage`: simple request/response. With `onMessage`:
   streaming — each backend channel message calls the callback, the
   promise resolves with the final return value when the handler
   completes.

   The host owns a single Tauri command (`plugin_message`) that
   creates the channel, dispatches to the plugin by ID, and bridges
   responses back to the frontend. This is WASM-friendly since the
   host controls the dispatch boundary.

A plugin can use either or both paths. A minimal plugin (emoji grid)
uses only path 1. A complex plugin (file browser) might use path 1
for initial results and path 2 for drill-down navigation.

### 5. Keyboard Ownership

*Who owns keyboard handling when a plugin UI is active? How do global
bindings coexist with plugin-local bindings?*

Status: **decided**

**The plugin owns all keybindings when active.** The host deregisters
its navigation and action bindings (arrows, PageUp/Down, Enter) when
a plugin component is mounted. The plugin registers everything it
needs through the existing keybinding engine.

This includes:
- Navigation (arrows, Tab, PageUp/Down — whatever suits the layout)
- Action execution (Enter for primary, modifier combos for secondary)
- Escape (with `goBack()` fallback when nothing internal to unwind)

The plugin is responsible for:
- Binding all keys it advertises in the footer
- Calling `onExecute(entryId, actionId)` when an action is triggered
- Keeping `onFooterChange` in sync with actual bindings

**Host keeps active regardless of plugin state:**
- Emacs input bindings (Ctrl+W/U/K/A/E)
- Global shortcut (toggle launcher visibility)

### 6. Shared Component Library / SDK Surface

*What host components and utilities are available to plugins? How are
they delivered?*

Status: **decided**

Plugins import from stable `@torchsnap/*` path aliases that map to
internal modules. These paths are the future external package names —
when the SDK is extracted, only the alias resolution changes; plugin
imports stay the same.

Initial modules:

- `@torchsnap/keybindings` → `./src/keybindings`
  (useKeyBindings, KeyCombo types, matching utilities)
- `@torchsnap/components` → `./src/components`
  (KeyPill, Switch, SettingsEntry, and other shared UI)
- `@torchsnap/types` → `./src/launcher/types`
  (ScoredEntry, EntryIcon, ActionId, FooterState, etc.)

Aliases are configured in both `tsconfig.json` (for type checking)
and Vite `resolve.alias` (for build resolution).

Domain-specific hooks live in their domain module (e.g.,
`useKeyBindings` is part of `@torchsnap/keybindings`, not a
separate `@torchsnap/hooks`). Additional modules are added as
needed when new plugin-public surface area emerges.

For internal plugins (current phase), these are just aliased
imports. No package extraction, no build boundary. The aliases
establish the contract and make future extraction mechanical.

### 7. Footer Integration

*Does the plugin control what the footer shows, or does the host derive
it from the selected entry?*

Status: **decided**

The footer uses a generic `FooterState` model:

```typescript
type FooterHint = {
    combo?: KeyCombo;
    label: string;
};

type FooterState = {
    primary?: FooterHint;
    hints: FooterHint[];
};
```

Both the host and plugins produce `FooterState`:

- **Host (list mode):** Derives `FooterState` from the selected
  entry's actions — maps each action to a `FooterHint` with its
  declared keybinding and label.
- **Plugin (custom UI):** Sets `FooterState` directly via the
  `onFooterChange` callback. The plugin controls exactly what the
  footer displays, keeping it in sync with its actual keybindings.

The footer component only knows about `FooterState`, never about
entries or actions. This decouples footer rendering from the result
model and ensures consistency regardless of who provides the data.

## Deferred Items

- **Dynamic plugin component registration:** The frontend currently
  uses a static map (plugin ID → React component). This will be
  replaced with dynamic resolution once plugin files and dynamic
  loading are designed (Phase 3). See todo.
- **Plugin message bus:** The `sendMessage` / `handle_message`
  mechanism (topic 4) is stubbed. The trait method defaults to an
  error. The Tauri command and frontend wrapper are built when the
  first plugin needs custom backend communication. See todo.
- **Emoji frecency for empty query:** The empty-prefix query (just
  `:`) shows unfiltered emoji in emojibase order for now. Will be
  replaced with frecency-ranked results once the ranking system
  exists. See todo.

## Decision

Plugins can provide React components that render into the launcher's
result list area. The host controls the shell (search bar, card chrome,
footer). Activation is signaled by the backend through search results
(prefix path) or execute return value (future drill-in path). The
plugin owns all keybindings when active and communicates footer state
and execution requests back to the host. Data flows through
host-provided results and optionally through a direct message bus.
Shared components are imported via stable `@torchsnap/*` path aliases.

## Consequences

- Plugins get full rendering control within a bounded area, enabling
  layouts like emoji grids, calculator inlines, and preview panes
  without host-side display mode enumerations.
- The footer refactoring (entry actions → generic `FooterState`) is
  a prerequisite and improves the footer's generality regardless of
  plugin custom UI.
- The host's keybinding registration must become conditional —
  navigation/action bindings are skipped when a plugin is mounted.
- Internal plugins use direct imports via path aliases; the extraction
  to real packages is deferred but mechanical when needed.
- The static plugin ID → component map is a known coupling point,
  acceptable for internal plugins, to be replaced with dynamic
  resolution in Phase 3.
