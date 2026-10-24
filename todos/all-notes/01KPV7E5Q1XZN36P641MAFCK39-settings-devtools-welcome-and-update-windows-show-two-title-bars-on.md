---
title: "Settings, devtools, welcome and update windows show two title bars on Linux"
kind: bug
component: platform
status: needs-discussion
impact: high
horizon: next
---

# Settings, devtools, welcome and update windows show two title bars on Linux

On Fedora 43 with GNOME, the settings and devtools windows show GNOME's
native title bar and the in-webview `<TitleBar />` at the same time. The
React layout also reserves its own strip, so the window has two title
strips. The launcher window is unaffected because it is built with
`.transparent(true).decorations(false)`.

Seen again on 2026-10-24 on the Raspberry Pi testbed (Raspberry Pi OS,
GNOME): the windows carry the desktop's own title bar while the page
paints the macOS-style one. The maintainer: a way must be found so these
windows look right on Linux too, and this must be done before the first
Linux release.

All four auxiliary windows are affected, since all four are built with
`hide_native_chrome: true` and render `<TitleBar />`: settings
(`src/settings/SettingsPanel.tsx`), developer tools
(`src/devtools/DevToolsPanel.tsx`), welcome (`src/welcome/WelcomeWindow.tsx`,
`closable={false}`) and software update (`src/update/UpdateWindow.tsx`).
Under option B below the welcome window would get the desktop's close
button, which its `closable={false}` withholds today; that needs a
decision of its own.

## Why it happens

ADR 0034 documents the cross-platform contract: auxiliary windows keep
their native window, and a platform-specific
`PlatformWindowChrome::hide_controls(&window)` strips only the window
controls (traffic lights on macOS, equivalents elsewhere) so the frontend
can paint its own title bar without native buttons getting in the way.

The macOS implementation uses AppKit's public
`NSWindow.standardWindowButton(_:)` and `NSButton.setHidden(true)` to hide
only the three buttons while leaving shadow, rounded corners and
edge-drag resize intact. It does this because Tauri's `decorations(false)`
removes too much on macOS.

On Linux the implementation in
`src-tauri/src/platform/fallback/window_chrome.rs` is a
`FallbackWindowChrome` no-op:

```rust
impl WindowChrome for FallbackWindowChrome {
    fn hide_controls(_window: &tauri::WebviewWindow) -> anyhow::Result<()> {
        Ok(())
    }
}
```

So on Linux the native GNOME title bar is never touched, and the
`<TitleBar />` overlay paints on top of it.

## Options

### A. Hide the native decorations on Linux

GTK exposes `gtk_window_set_decorated(window, false)`, which removes the
native title bar entirely. Tauri's `WebviewWindow` gives access to the
underlying `GtkWindow` via `window.gtk_window()` (behind the `gtk` feature
on Linux), so a real `LinuxWindowChrome` is a few lines.

Unlike macOS, the cost side is manageable:

- Rounded corners: GNOME does not draw window corners through the OS.
  They are a CSS effect of the app's own chrome, which we already own in
  React, so this is neutral.
- Shadow: provided by the Wayland compositor / Mutter independently of
  `decorated`. Neutral.
- Edge-drag resize: lost. With `decorated(false)` on Wayland the
  compositor no longer serves resize edges. The app has to call
  `xdg_toplevel::resize` itself, which in Tauri means wiring
  `data-tauri-drag-region` equivalents for eight edges. Non-trivial.

Feasible, but it means writing and maintaining our own resize affordance.

### B. Use the native chrome on Linux, drop our custom TitleBar

Instead of fighting CSD, accept that the native GNOME header bar is the
title bar on Linux, and teach the frontend to omit `<TitleBar />` (or
return null from it) when the platform dispatcher reports Linux.
Concretely:

- `<TitleBar />` gains a Linux branch that renders `null` (or a
  zero-height spacer, whichever the layout prefers).
- The 48px/32px vertical space the component reserves via its strip has
  to be removed from the settings and devtools layouts on Linux. For
  settings, the sidebar card starts at the window's top edge. For
  devtools, the window title moves from the in-frontend `"titlebar"`
  variant's centred label to the native `.title("Developer Tools")` the
  window already has.
- Tauri's built-in drag region is not needed, since GNOME handles window
  drag via the native header bar.
- Native minimize, maximize and close buttons are back in play. The
  JS-side calls to `.close()`, `.minimize()` and `.setFullscreen(...)`
  become unnecessary on this platform (not harmful, just redundant).

This is less code, aligns with GNOME's CSD philosophy, and sidesteps the
resize-handle problem entirely. The cost is a visual divergence from
macOS and Windows, but that is already the platform's own convention.

### C. Hybrid: decorated(false) plus our own resize handles

Write the full resize-edge machinery in React and reuse the same
`<TitleBar />` overlay on Linux. This gives pixel-identical chrome across
platforms, at the highest code and maintenance cost of the three. Probably
not worth it unless cross-platform visual identity is a non-negotiable
product requirement.

## Recommendation worth discussing

Option B is the smallest diff and the most platform-idiomatic. It only
loses cross-platform visual parity, which the project does not hold
sacred elsewhere (the tray-icon platform differences, the launcher
panel's macOS-specific NSPanel versus the fallback window, and so on).

If we still want parity later, option A can be added on top. The
`WindowChrome` abstraction is already there, and writing the GTK call is
the easy part. The hard part is the resize handles, which can wait until
there is a concrete reason to own them.

This needs a decision, and an ADR update that extends ADR 0034 with the
Linux verdict.

## Pointers

- `src-tauri/src/lib.rs` lines 190 to 227: `show_auxiliary_window`, the
  `hide_native_chrome` flag and the `TitleBarStyle::Overlay` path.
- `src-tauri/src/platform/mod.rs`: `WindowChrome` trait declaration.
- `src-tauri/src/platform/fallback/window_chrome.rs`: current no-op Linux
  implementation.
- `src-tauri/src/platform/macos/window_chrome.rs`: reference macOS
  implementation.
- `docs/adr/0034-custom-title-bar-for-auxiliary-windows-with-native-controls-hidden.md`:
  original cross-platform contract.
- `src/components/TitleBar/`: frontend component that would gain the
  Linux branch under option B.
