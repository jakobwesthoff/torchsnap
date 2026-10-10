---
title: "Decide whether the Linux launcher can realize its webview while hidden"
kind: investigation
component: platform
---

# Decide whether the Linux launcher can realize its webview while hidden

On Linux the launcher window is built visible, and it stays visible until
the user first dismisses it. The goal is to start hidden, as on macOS,
without losing the instant first hotkey press. That needs WebKitGTK to
realize the webview (run the JS, mount React, drain `launcher_set_layout`)
without ever mapping the GtkWindow. This note records the approaches to
try.

## Context

`src-tauri/src/lib.rs` currently builds the launcher window with
`#[cfg(not(target_os = "linux"))] .visible(false)`. On macOS and Windows
the window is built hidden, so the hotkey-driven summon feels instant and
there is no startup flash. On Linux the window is built visible and stays
so until the user dismisses it the first time.

The Linux branch exists because **WebKitGTK does not run the webview (and
therefore never mounts React) while the host GtkWindow is unmapped.** The
`launcher_set_layout` handshake that gates every `show` path would
otherwise never arrive, producing the failure mode debugged on Fedora 43:
every show, toggle and control call bails with
`"launcher layout not yet received from frontend"`.

`WKWebView` on macOS loads content regardless of the host window's
visibility, which is why the "build hidden, wait for layout, summon on
demand" architecture works there.

The current Linux behaviour is a pragmatic compromise. The launcher is
visible on the desktop after startup, the user dismisses it once, and from
that moment the session behaves macOS-like, because GTK's `hide()` unmaps
without destroying the realized widget tree. The cost is a visible
launcher window for the time between app launch and the user's first
dismiss.

## Options

1. Explicit widget realize. Tauri v2's `WebviewWindow` exposes
   `gtk_window()` on Linux (behind the `gtk` feature, which `wry` enables
   by default). After building the window hidden, call
   `gtk_window()?.realize()` on it. GTK realization allocates the widget
   hierarchy and, for WebKitGTK, should trigger the `WebKitWebView` to
   instantiate its renderer process and begin loading the assigned URI,
   without mapping the window onto the display.

   Investigation points:
   - Does `WebKitWebView::load_uri` actually fire when the view is
     realized but not mapped? This is the crux. Realization and mapping
     are distinct stages in GTK, but WebKitGTK may be lazy about kicking
     off the renderer until the first `map` event.
   - If realize alone is insufficient, try `gtk_widget_map(webview_widget)`
     followed immediately by `gtk_widget_unmap(webview_widget)`. Some
     WebKitGTK consumers use that trick.

2. Offscreen GtkOffscreenWindow as host. As a fallback, parent the webview
   inside a `GtkOffscreenWindow` during warm-up, then reparent it into the
   real `GtkWindow` once layout is reported. This is more invasive and
   probably not worth the complexity if option 1 works.

3. Hide immediately after layout. If we cannot avoid mapping but want to
   minimize the visible flash, show the launcher at startup (the current
   state) and call `win.hide()` automatically from `launcher_set_layout`
   as soon as the layout arrives. The user would still see a brief flash
   on startup, but lasting milliseconds rather than until a user dismiss.
   This is a regression from the current "launcher stays up until first
   dismiss" behaviour, which some users may prefer. Worth user-testing
   before committing.

## Pointers

- `src-tauri/src/lib.rs`: launcher window builder with the
  `#[cfg(not(target_os = "linux"))]` visibility override.
- `src-tauri/src/lib.rs::launcher_set_layout`: the Tauri command the
  frontend invokes from `useEffect` on React mount. It is also the natural
  hook point for an automatic post-layout hide.
- `src-tauri/src/platform/fallback/launcher_panel.rs`: the
  `LauncherPanel::init` no-op that would gain a realize call under option 1.
- WebKitGTK docs on realize and map semantics:
  <https://webkitgtk.org/reference/webkit2gtk/stable/>

## Non-goals

- Fixing `position_launcher_on_cursor_monitor` on Wayland. That is a
  separate, larger issue (Wayland clients cannot set absolute window
  positions) and belongs in its own todo.
- Changing the launcher's summon UX on macOS. The hidden-prewarm
  architecture stays as it is on macOS.
