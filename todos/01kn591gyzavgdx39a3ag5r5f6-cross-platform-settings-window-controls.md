# Cross-Platform Window Controls for Settings Window

## Current State

The settings window uses macOS-specific `TitleBarStyle::Overlay` +
`hidden_title(true)` for its titlebar (`src-tauri/src/lib.rs:131-136`). This
renders native traffic lights on macOS with an invisible drag region across the
top 48px. Other platforms would get no window controls at all since this is
guarded by `#[cfg(target_os = "macos")]`.

## Problem

When we eventually target Windows and Linux, the settings window needs
functional close/minimize/maximize controls. We need to decide on an approach
before porting.

## Possible Approaches

### 1. `tauri-controls` library (evaluated, not chosen)

**Repo:** https://github.com/agmmnn/tauri-controls
**What it does:** Renders pixel-accurate HTML/CSS replicas of native window
controls (macOS traffic lights, Windows 11 buttons, GNOME buttons). Wires them
to Tauri's JS window API. Supports React, Solid, Vue, Svelte.

**How it works:**
- Requires `decorations: false` on the Tauri window (removes native titlebar)
- Auto-detects OS via `@tauri-apps/plugin-os` `type()` to pick the right style
- Ships pre-compiled CSS (Tailwind v3 build, self-contained, auto-imported)
- Exports `WindowControls` (just buttons) and `WindowTitlebar` (buttons + drag region wrapper)
- macOS controls detect Alt key for fullscreen vs. zoom toggle
- Windows controls track maximized state for icon swap

**Problems identified:**
- Ships a full Tailwind v3 Preflight (CSS reset) alongside our v4 one — potential subtle conflicts
- Pre-compiled dark mode uses `.dark` class selector, but we use `data-theme="dark"` — requires bridging
- Requires `decorations: false` which strips native window chrome entirely on *all* platforms — on macOS we lose real traffic lights, rounded corners, and window shadow (would need NSWindow hacks to restore)
- React peer dep is `^18.2.0`, we use React 19 (works but warns)
- `tailwind-merge` peer dep is `^1.14.0`, we use v3 (works but warns)
- Adds complexity for what is ultimately three buttons

### 2. Keep native titlebar per platform (simplest)

Use the current macOS overlay titlebar as-is. On Windows/Linux, just use the
default native `decorations: true` titlebar. The settings window would look
slightly different per platform but fully native. Minimal code, zero
maintenance burden.

Trade-off: less design control on Windows/Linux, can't style the titlebar area.

### 3. Custom minimal implementation (middle ground)

Implement our own close/minimize/maximize buttons using Tauri's
`@tauri-apps/api/window` directly. Only the subset we actually need:
- Three SVG icon buttons styled with our existing design tokens
- Platform detection to position left (macOS) vs. right (Windows/Linux)
- Could conditionally use native titlebar on macOS (`TitleBarStyle::Overlay`)
  and custom controls only on platforms where we need them

Trade-off: more code to maintain, but fully under our control with no
third-party CSS conflicts.

## Open Questions

- Do we want consistent cross-platform appearance, or is per-platform native
  chrome acceptable?
- On macOS, the current overlay titlebar works well — is there value in
  replacing it with custom controls, or should we only add custom controls on
  platforms that need them?
- How much design control do we need over the titlebar area on non-macOS
  platforms?

## Decision

Needs further discussion and evaluation once we begin targeting Windows/Linux.
