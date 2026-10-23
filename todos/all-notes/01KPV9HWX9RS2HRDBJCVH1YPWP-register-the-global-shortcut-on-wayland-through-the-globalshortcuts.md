---
title: "Register the global shortcut on Wayland through the GlobalShortcuts portal"
kind: bug
component: platform
impact: high
---

# Register the global shortcut on Wayland through the GlobalShortcuts portal

On a Wayland session (Fedora 43 with GNOME 46+ by default) the in-app
global shortcut never fires from normal focus. The fix is to register the
shortcut through the XDG `GlobalShortcuts` portal on Wayland, and keep the
X11 path for pure X11 sessions.

## Problem

`tauri-plugin-global-shortcut` depends on the `global-hotkey` crate, whose
Linux backend is X11-only. It installs an `x11rb`/`XGrabKey` grab against
XWayland's X server. Mutter does not forward key events to XWayland
grabbers when a Wayland-native window holds focus, so the grab is
effectively unreachable for most users.

Verified upstream: `~/.cargo/registry/src/index.crates.io-*/global-hotkey-0.8.0/src/platform_impl/`
routes Linux to `x11/mod.rs` (x11rb `grab_key`). No Wayland or portal
backend exists.

Users currently rely on a GNOME custom keybinding that invokes the Control
API over `socat`, documented in `docs/Howto-build-on-fedora-43.md` under the
troubleshooting heading *"Global shortcut does nothing on Wayland"*. That
workaround is fine to ship today and stays in the docs until this todo
lands.

## Decision

Implement the `org.freedesktop.portal.GlobalShortcuts` integration locally
and do not wait for upstream. Ship it as a Linux-only platform module
alongside the existing `LauncherPanel`, `WindowChrome` and `Tray`
abstractions.

- **Wayland sessions** (`$WAYLAND_DISPLAY` set): register via the portal
  through `ashpd`.
- **Pure X11 sessions** (e.g. `GDK_BACKEND=x11 just start`, Xfce spins):
  keep using `tauri-plugin-global-shortcut` / `global-hotkey`, so the X11
  path keeps working.
- **macOS and Windows**: unchanged.

## Implementation sketch

### Dependency

```toml
# src-tauri/Cargo.toml
[target.'cfg(target_os = "linux")'.dependencies]
ashpd = { version = "0.13", default-features = false, features = ["tokio", "global_shortcuts"] }
```

`ashpd` 0.13.10 exposes the portal under `ashpd::desktop::global_shortcuts`
with a safe async API over `zbus`. The `tokio` feature slots into the
existing tokio runtime this project already uses.

### Platform module

`src-tauri/src/platform/linux/global_shortcut.rs` (a new module; promote the
current `fallback` into a proper `linux` peer of `macos/`):

- `bind_shortcuts(session, specs)` at startup, with `(id, description,
  preferred_trigger)` tuples. The launcher's combo becomes one of them, and
  gadget-declared shortcuts are added with their gadget id as the
  portal-side id.
- A `receive_activated()` stream, dispatched into `gadget_host.rs::on_shortcut`
  equivalents. Preserve the existing launcher-toggle-versus-gadget routing.
- A `receive_shortcuts_changed()` stream that writes the new combos back into
  the settings store, so the in-app UI reflects reality.
- Hold the portal Session object for the lifetime of the app. If it drops,
  the bindings disappear.

### Registration dispatch

`gadget_host.rs::register_all_shortcuts` gains a cfg split:

- `cfg(all(target_os = "linux", "wayland detected at runtime"))`: portal module.
- `cfg(not(target_os = "linux"))`, or pure-X11 Linux: `tauri_plugin_global_shortcut`
  as today.

Runtime detection: `std::env::var("WAYLAND_DISPLAY").is_ok()`.

## Settings UI impact

The in-app *Global Shortcut* picker becomes meaningless on Wayland. The
portal owns the mapping, and any change the user makes in the app would be
overwritten or silently ignored by the compositor. Hide it.

Concretely:

- **Detect the portal path**: add a small Tauri command that returns whether
  shortcuts are portal-managed on this session.
- **Wayland branch of the settings pane**: replace the picker with a short
  note:

  > *Torchsnap's global shortcut is managed by your desktop environment.
  > Open GNOME Settings → Keyboard → Keyboard Shortcuts to view or change it.*
  > *[Open Keyboard Settings]* (button invoking `gnome-control-center keyboard`
  > or `gio launch /usr/share/applications/gnome-control-center.desktop keyboard`).

- On non-Wayland Linux the picker stays, since it still works via the X11
  grab path.
- The wording stays compositor-neutral ("your desktop environment") because
  KDE Plasma and wlroots-based compositors also expose the portal-bound
  shortcuts in their own keyboard settings pane.

### Where the user actually goes

- GNOME 45+: Settings → Keyboard → *View and Customize Shortcuts*. A grouped
  section *Global Shortcuts* lists each app that called `bind_shortcuts` by
  its portal id.
- KDE Plasma: System Settings → Shortcuts, listed per app.
- Other Wayland compositors whose portal backend implements
  `GlobalShortcuts`: their respective keyboard config UIs, if any.
  `xdg-desktop-portal-wlr` does not implement it (see *labwc*).

## labwc

On Raspberry Pi OS (Debian 13) with labwc 0.20.2 and
`xdg-desktop-portal-wlr` 0.7.1 as the preferred portal backend, the
portal exposes no `org.freedesktop.portal.GlobalShortcuts` interface
(`busctl --user introspect org.freedesktop.portal.Desktop
/org/freedesktop/portal/desktop`, 2026-10-23). The portal path does not
cover labwc. There the shortcut has to come from a `<keybind>` in
`~/.config/labwc/rc.xml` that calls the control socket, like the
documented GNOME workaround. Not checked: other wlroots compositors
with a portal backend of their own.

## UX notes worth internalising before coding

- **First-run consent dialog.** The compositor shows a modal on the first
  `bind_shortcuts`, listing every shortcut the app wants with the suggested
  combos. The user can accept, customise or reject. Surface this in onboarding
  copy so it does not read as a security warning.
- **The app proposes, the user disposes.** `preferred_trigger` is a
  suggestion; the compositor-negotiated combo is the real one. Always read
  back the result of `bind_shortcuts` rather than assuming the preferred
  trigger was honoured.
- **`configure_shortcuts`** exists and lets the app request a new trigger, but
  it typically prompts the user again. Use it sparingly, for example if the
  user renames the shortcut in the gadget list.
- **Session lifecycle.** Dropping the session makes the bindings vanish. Tie
  its lifetime to the app's main `run_loop`.

## Non-goals

- Removing `tauri-plugin-global-shortcut` entirely. Keep it for Windows, macOS
  and X11-session Linux.
- Supporting compositors without a `GlobalShortcuts` portal backend, such
  as labwc and other compositors on `xdg-desktop-portal-wlr`. They fall
  through to the X11 path and live with the same Control API workaround that
  is documented today.

## Pointers

- `docs/Howto-build-on-fedora-43.md`: user-facing documentation of the current
  Control API socket workaround. Remove or rewrite it when this todo lands.
- `src-tauri/src/gadget_host.rs`, `register_all_shortcuts`: shortcut
  registration fan-out, where the dispatch split lives.
- `src-tauri/src/lib.rs`, `run()`: the `.plugin(tauri_plugin_global_shortcut::...)`
  registration. Keep it, and stop using it on Wayland-Linux.
- `src-tauri/src/platform/fallback/`: rename to `linux/` when this lands, to
  match `macos/` and house `global_shortcut.rs` there.
- `docs/control-api.md`: the socket used by the current workaround.
- ashpd docs: <https://docs.rs/ashpd/0.13.10/ashpd/desktop/global_shortcuts/>
- Portal spec: <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html>
