---
kind: bug
status: open
---

# Linux: auxiliary windows render with native chrome + custom TitleBar

On Fedora 43 / GNOME, the **settings** and **devtools** windows ship
with full GNOME window decorations (client-side title bar with
close / minimize / maximize buttons, rounded corners, shadow) *on top
of which* our in-webview `<TitleBar />` component is also rendered.
The result is double chrome: a native GNOME title bar and a second
title strip reserved by the React layout underneath it.

The launcher window is unaffected — it is built with
`.transparent(true).decorations(false)` (see the launcher
`WebviewWindowBuilder` in `src-tauri/src/lib.rs`) and has no
auxiliary-window issue.

## Why it happens

ADR 0034 documents the cross-platform contract: auxiliary windows keep
their native window, and a platform-specific
`PlatformWindowChrome::hide_controls(&window)` strips only the window
controls (traffic lights on macOS, equivalents elsewhere) so the
frontend can paint its own title bar without native buttons getting
in the way.

The macOS implementation uses AppKit's public
`NSWindow.standardWindowButton(_:)` + `NSButton.setHidden(true)` to
hide only the three buttons while leaving shadow, rounded corners,
and edge-drag resize intact — precisely because Tauri's
`decorations(false)` removes too much on macOS.

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

### A — Hide the native decorations on Linux

GTK exposes `gtk_window_set_decorated(window, false)` which completely
removes the native title bar. Tauri's `WebviewWindow` gives access to
the underlying `GtkWindow` via `window.gtk_window()` (behind the
`gtk` feature on Linux), so a real `LinuxWindowChrome` is a few lines.

Unlike macOS, the cost side is manageable:

- Rounded corners: GNOME does not draw window corners through the OS,
  they are a CSS effect of the app's own chrome — we already own the
  chrome in React, so this is neutral.
- Shadow: provided by the Wayland compositor / Mutter independently
  of `decorated`. Neutral.
- **Edge-drag resize: lost.** With `decorated(false)` on Wayland the
  compositor no longer serves resize edges. The app has to call
  `xdg_toplevel::resize` itself, which in Tauri means wiring
  `data-tauri-drag-region` equivalents for eight edges. Non-trivial.

Feasible but means writing and maintaining our own resize affordance.

### B — Use the native chrome on Linux, drop our custom TitleBar

Instead of fighting CSD, accept that the native GNOME header bar is
the title bar on Linux and teach the frontend to omit `<TitleBar />`
(or return null from it) when the platform dispatcher reports Linux.
Concretely:

- `<TitleBar />` gains a Linux branch that renders `null` (or a
  zero-height spacer, whichever the layout prefers).
- The 48px/32px vertical space the component currently reserves via
  its strip has to be removed from the settings + devtools layouts
  on Linux. For settings this means the sidebar card starts at the
  window's top edge; for devtools the window title moves from the
  in-frontend `"titlebar"` variant's centred label to the native
  `.title("Developer Tools")` the window already has.
- Tauri's built-in drag region is not needed — GNOME handles window
  drag via the native header bar.
- Native minimize / maximize / close buttons are back in play; the
  JS-side calls to `.close() / .minimize() / .setFullscreen(...)`
  become unnecessary on this platform (not harmful, just redundant).

This is less code, aligns with GNOME's CSD philosophy, and sidesteps
the resize-handle problem entirely. The cost is a visual divergence
from macOS/Windows, but that's already the platform's own convention.

### C — Hybrid: decorated(false) + our own resize handles

Write the full resize-edge machinery in React and reuse the same
`<TitleBar />` overlay on Linux. Gives us pixel-identical chrome
across platforms. Highest code + maintenance cost of the three;
probably not worth it unless cross-platform visual identity is a
non-negotiable product requirement.

## Recommendation worth discussing

Option B is the smallest diff and the most platform-idiomatic. It
only loses cross-platform visual parity, which the project does not
hold sacred elsewhere (the tray-icon platform differences, the
launcher panel's macOS-specific NSPanel vs fallback window, etc.).

If we still want parity later, option A can be added on top — the
`WindowChrome` abstraction is already there; writing the GTK call is
the easy part. The hard part is the resize handles, and that can
wait until we have a concrete reason to own them.

Needs a decision + an ADR update that extends 0034 with the Linux
verdict.

## Pointers

- `src-tauri/src/lib.rs:190-227` — `show_auxiliary_window`, the
  `hide_native_chrome` flag, and the `TitleBarStyle::Overlay` path
- `src-tauri/src/platform/mod.rs` — `WindowChrome` trait declaration
- `src-tauri/src/platform/fallback/window_chrome.rs` — current
  no-op Linux implementation
- `src-tauri/src/platform/macos/window_chrome.rs` — reference
  macOS implementation
- `docs/adr/0034-custom-title-bar-for-auxiliary-windows-with-native-controls-hidden.md`
  — original cross-platform contract
- `src/components/TitleBar/` — frontend component that would gain
  the Linux branch under option B
