# 34. Custom title bar for auxiliary windows with native controls hidden

Date: 2026-04-11

## Status

Accepted

## Context

The settings window (and eventually the devtools window) is an auxiliary
Tauri window that needs to look and feel native to the host OS. On macOS
we had been using `TitleBarStyle::Overlay` + `hidden_title(true)` — the
standard Tauri recipe for a content-through-titlebar window. It keeps the
three traffic lights visible and overlays them on top of the webview at
their native position.

Two problems with that recipe pushed us to look for something else:

1. **The traffic lights are pinned to the window's top-left corner by
   AppKit.** We want the settings window to be laid out with a sidebar
   card that has equal margins on all four sides of the window. With the
   native traffic lights at their native position, they land outside the
   sidebar card in an awkward gap — or forced us to drop the left margin
   on the sidebar to encase them, which looked wrong. There is no Tauri
   API (nor public AppKit one) to reposition `standardWindowButton`s, so
   we cannot move them to wherever the card happens to be.

2. **Cross-platform consistency.** We plan to ship on Windows (and
   eventually Linux), and the native window chrome on those platforms
   looks nothing like macOS. Rather than writing three different layout
   passes that react to whatever the native title bar gave us, we would
   rather render the whole title area ourselves — drag region, window
   controls, and any ancillary chrome — and have a single layout the
   platform layer merely plugs into. That requires getting the native
   chrome out of the way on every platform we support.

### Research: can Tauri hide the traffic lights natively?

Before going platform-specific we researched Tauri v2's public API
surface. The findings, each with primary sources:

- **`TitleBarStyle`** (`Visible`, `Transparent`, `Overlay`) is the only
  macOS-specific title bar knob Tauri exposes. None of the three variants
  hides the traffic light buttons. See `crates/tauri-utils/src/lib.rs`
  on the `dev` branch.

- **`hidden_title(true)`** hides only the *title text* of the window,
  not the buttons — confirmed by the Tauri source comment on that
  builder method.

- **`decorations(false)`** removes the native title bar entirely. On
  macOS it also removes the native rounded window corners and (by
  default) the native drop shadow. Shadow can be restored with
  `.shadow(true)`; rounded corners cannot be restored through any Tauri
  API. Tauri issue
  [#12042](https://github.com/tauri-apps/tauri/issues/12042) explicitly
  tracks this inconsistency: `decorations(false)` on macOS strips more
  than on Windows.

- **Community plugins**: `tauri-plugin-decorum` only *repositions* the
  traffic lights, it does not hide them. `tauri-plugin-mac-rounded-corners`
  can restore native rounded corners after `decorations(false)` via
  public AppKit API, but it pulls in an additional macOS-only crate and
  still leaves us doing platform-specific work outside our own codebase.

- **Transparency + CSS `border-radius`** is another community workaround
  that emulates rounded corners by making the webview transparent and
  clipping the root DOM element. It works, but it means the window has no
  native shape at all and any flash before React paints leaks through to
  the desktop. Rejected as hackier than the alternative.

The binding constraint: **Tauri v2 has no API that hides only the
traffic lights while keeping macOS rounded corners, shadow, and edge-drag
resize intact.** Every path that gets rid of the traffic lights via
`decorations(false)` strips too much on macOS and has no clean recovery.

## Decision

We render the entire title bar (drag region plus window controls)
ourselves in the frontend via a dedicated `<TitleBar />` component that
dispatches to a platform-specific implementation, and we hide the native
controls through a new platform abstraction on the Rust side.

Concretely:

- **Rust side.** A new `WindowChrome` trait in
  `src-tauri/src/platform/mod.rs` exposes a single method,
  `hide_controls(window: &WebviewWindow)`. The macOS implementation
  (`src-tauri/src/platform/macos/window_chrome.rs`) walks the three
  `NSWindowButton` variants (`CloseButton`, `MiniaturizeButton`,
  `ZoomButton`) via the public `NSWindow.standardWindowButton(_:)` API
  and calls `setHidden(true)` on each returned `NSButton`. The fallback
  implementation is a no-op — when we add a real Windows or Linux
  backend, the fallback is replaced with the corresponding platform
  module.

  `AuxiliaryWindowConfig` gains a `hide_native_chrome: bool` flag.
  `show_auxiliary_window` invokes `PlatformWindowChrome::hide_controls`
  on the freshly built window when the flag is set. The settings window
  opts in; the devtools window does not (for now).

  `TitleBarStyle::Overlay` + `hidden_title(true)` stay in place on
  macOS. They continue to give us the transparent title bar region
  underneath which the webview renders, and the rest of the native
  window (shadow, rounded corners, resize) is fully intact because we
  never touched `decorations`.

- **Frontend side.** A `TitleBar` component in `src/components/` owns
  the title bar completely. It renders absolutely over the top of the
  main layout so swapping it for a different platform implementation
  does not perturb the layout underneath. It contains:
  - A `data-tauri-drag-region` strip spanning the full window width.
  - Platform-appropriate window controls (macOS: three traffic-light
    circles calling `getCurrentWindow().close() / .minimize() /
    .setFullscreen(!)`).

  The component dispatches internally to `MacTitleBar` for now. When we
  add Windows, a `WinTitleBar` sibling is added and the dispatch
  branches. Because the component is the only thing that knows its own
  height, adding a platform whose title bar is a different size does
  not break the underlying layout — only the chrome overlay changes.

### Alternatives considered

- **`decorations(false) + shadow(true)`, cross-platform.** Rejected
  because it loses native rounded corners on macOS with no clean
  restore via Tauri API. We would either accept a non-native square
  window or add a macOS corner-recovery plugin. Neither was appealing:
  the first visibly breaks macOS feel, the second adds an external
  maintenance burden for something we can solve in ten lines of
  platform-local Rust.

- **Keep the native traffic lights, design the layout around them.**
  Rejected because the traffic lights' fixed position fights our
  desired sidebar-with-equal-margins layout, and it does not help us
  with the cross-platform story at all.

- **`tauri-plugin-decorum`.** Rejected — it repositions, not hides,
  and is in maintenance mode.

- **Transparent window + CSS border-radius.** Rejected as
  substantively hackier than an AppKit call via `objc2`: it requires
  the whole webview to be transparent, which trickles into flash
  prevention, click-through behavior in corner regions, and every
  future platform having to deal with the same workaround.

## Consequences

### Positive

- **Native window feel preserved.** Shadow, rounded corners, edge-drag
  resize all continue to work unchanged. Only the three buttons
  disappear.

- **Layout is platform-agnostic.** The settings UI is free to place its
  sidebar card with equal margins without fighting AppKit over where
  the traffic lights live. When the Windows and Linux implementations
  land, the layout does not need to change — only the `TitleBar`
  variant that overlays it.

- **Abstraction has one obvious extension point.** Adding Windows or
  Linux support means writing a new `platform/<os>/window_chrome.rs`
  that implements the trait and a new frontend `<WinTitleBar>` /
  `<LinuxTitleBar>` that the dispatcher can branch to. No other code
  needs to change.

- **We stay on public API.** `NSWindow.standardWindowButton(_:)` and
  `NSButton.setHidden(_:)` have been documented AppKit since the
  early 2000s. Nothing here depends on private selectors or
  undocumented behavior — it is just an API Tauri v2 does not happen
  to wrap.

### Negative

- **Rust-side platform code.** We own a small amount of `objc2`
  interop in `platform/macos/window_chrome.rs`. This is already the
  pattern used for `launcher_panel.rs` and `platform/macos/mod.rs`, so
  the cost is marginal, but it does mean future contributors touching
  that file need to understand AppKit at a surface level.

- **Frontend has to reimplement macOS button affordances.** The custom
  traffic lights need to match macOS behavior (color on focus, gray on
  blur, hover icons, close / miniaturize / zoom semantics). Getting
  this pixel-close is more work than letting AppKit do it. The payoff
  is that the same component infrastructure serves Windows and Linux
  when those platforms land.

- **Per-window opt-in, temporary inconsistency.** The devtools window
  keeps its native chrome until its frontend is migrated to render
  `<TitleBar />`, at which point its `hide_native_chrome` flag flips
  to `true` and the whole app speaks one visual language again. Until
  that follow-up lands we live with two different window-chrome shapes
  in the app. Acceptable — the settings window is the one that drives
  the visual direction, and migrating devtools is mechanical once the
  component exists.
