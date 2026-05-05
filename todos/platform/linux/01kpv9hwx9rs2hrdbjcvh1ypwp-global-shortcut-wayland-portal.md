# Wayland-native global shortcut via XDG `GlobalShortcuts` portal

## Problem

On a Wayland session (Fedora 43 / GNOME 46+ by default), the in-app
global shortcut never fires from normal focus. `tauri-plugin-global-shortcut`
depends on the `global-hotkey` crate, whose Linux backend is X11-only
— it installs an `x11rb`/`XGrabKey` grab against XWayland's X server.
Mutter does not forward key events to XWayland grabbers when a
Wayland-native window holds focus, so the grab is effectively
unreachable for most users.

Verified upstream: `~/.cargo/registry/src/index.crates.io-*/global-hotkey-0.7.0/src/platform_impl/`
routes Linux to `x11/mod.rs` (x11rb `grab_key`). No Wayland or portal
backend exists.

Users currently rely on a GNOME custom keybinding that invokes the
Control API over `socat`, documented in
`docs/Howto-build-on-fedora-43.md` under the troubleshooting heading
*"Global shortcut does nothing on Wayland"*. That workaround is
intentionally ship-able today and stays in the docs until this todo
lands.

## Decision

Implement the `org.freedesktop.portal.GlobalShortcuts` integration
locally — don't wait for upstream. Ship it as a Linux-only platform
module alongside the existing `LauncherPanel` / `WindowChrome` /
`Tray` abstractions.

- **Wayland sessions** (`$WAYLAND_DISPLAY` set): register via the
  portal through `ashpd`.
- **Pure X11 sessions** (e.g. `GDK_BACKEND=x11 just start`, Xfce
  spins): keep using `tauri-plugin-global-shortcut` /
  `global-hotkey` so the X11 path stays working.
- **macOS / Windows**: unchanged.

## Implementation sketch

### Dependency

```toml
# src-tauri/Cargo.toml
[target.'cfg(target_os = "linux")'.dependencies]
ashpd = { version = "0.13", default-features = false, features = ["tokio", "global_shortcuts"] }
```

`ashpd` 0.13.10 exposes the portal under
`ashpd::desktop::global_shortcuts` with a safe async API over `zbus`.
The `tokio` feature slots into the existing tokio runtime this
project already uses.

### Platform module

`src-tauri/src/platform/linux/global_shortcut.rs` (new module,
promote the current `fallback` into a proper `linux` peer of
`macos/`):

- `bind_shortcuts(session, specs)` at startup with `(id, description,
  preferred_trigger)` tuples. The launcher's combo becomes one of
  them; gadget-declared shortcuts are added with their gadget id as
  the portal-side id.
- `receive_activated()` stream, dispatched into
  `gadget_host.rs::on_shortcut` equivalents. Preserve the existing
  launcher-toggle-vs-gadget routing.
- `receive_shortcuts_changed()` stream → write the new combos back
  into the settings store so the in-app UI reflects reality.
- Hold the portal Session object for the lifetime of the app — if
  it drops, bindings disappear.

### Registration dispatch

`gadget_host.rs::register_all_shortcuts` gains a cfg split:

- `cfg(all(target_os = "linux", "wayland detected at runtime"))`
  → portal module
- `cfg(not(target_os = "linux"))` or pure-X11 Linux
  → `tauri_plugin_global_shortcut` as today

Runtime detection: `std::env::var("WAYLAND_DISPLAY").is_ok()`.

## Settings UI impact

The in-app *Global Shortcut* picker becomes meaningless on Wayland
— the portal owns the mapping, and any changes the user makes in
the app would get overwritten (or silently ignored) by the
compositor. Hide it.

Concretely:

- **Detect the portal path**: add a small Tauri command that
  returns whether shortcuts are portal-managed on this session.
- **Wayland branch of the settings pane**: replace the picker with
  a short note:
  > *Torchsnap's global shortcut is managed by your desktop
  > environment. Open GNOME Settings → Keyboard → Keyboard
  > Shortcuts to view or change it.*
  > *[Open Keyboard Settings]* (button invoking
  > `gnome-control-center keyboard` or
  > `gio launch /usr/share/applications/gnome-control-center.desktop
  > keyboard`).
- On non-Wayland Linux the picker stays — it still works via the
  X11 grab path.
- The wording stays compositor-neutral ("your desktop environment")
  because KDE Plasma and wlroots-based compositors also expose the
  portal-bound shortcuts in their own keyboard settings pane.

### Where the user actually goes

- GNOME 45+: Settings → Keyboard → *View and Customize Shortcuts*
  → there is a grouped section *Global Shortcuts* that lists each
  app that called `bind_shortcuts` by its portal id.
- KDE Plasma: System Settings → Shortcuts → listed per app.
- Other Wayland compositors with portal backends (cosmic, hyprland,
  sway via `xdg-desktop-portal-wlr`): their respective keyboard
  config UIs, if any.

## UX notes worth internalising before coding

- **First-run consent dialog.** The compositor shows a modal on
  first `bind_shortcuts` listing every shortcut the app wants, with
  the suggested combos. The user can accept, customise, or reject.
  Surface this in onboarding copy so it doesn't read as a security
  warning.
- **The app proposes, the user disposes.** `preferred_trigger` is
  a suggestion; the compositor-negotiated combo is the real one.
  Always read back the result of `bind_shortcuts` rather than
  assuming the preferred trigger was honored.
- **`configure_shortcuts`** exists and lets the app request a new
  trigger, but typically prompts the user again — use sparingly
  (e.g. if the user renames the shortcut in the gadget list).
- **Session lifecycle.** Drop the session → bindings vanish. Tie
  its lifetime to the app's main `run_loop`.

## Non-goals

- Removing `tauri-plugin-global-shortcut` entirely. Keep it for
  Windows, macOS, and X11-session Linux.
- Supporting compositors without a portal backend. Minimal tiling
  setups without `xdg-desktop-portal-wlr` / equivalent are edge
  cases; fall through to the X11 path and let those users live
  with the same Control-API workaround we document today.

## Pointers

- `docs/Howto-build-on-fedora-43.md` — user-facing documentation
  of the current Control-API-socket workaround. Remove or rewrite
  when this todo lands.
- `src-tauri/src/gadget_host.rs:322-405` — shortcut registration
  fan-out, where the dispatch split lives.
- `src-tauri/src/lib.rs:526` — the
  `.plugin(tauri_plugin_global_shortcut::...)` registration; keep
  it, just stop using it on Wayland-Linux.
- `src-tauri/src/platform/fallback/` — rename to `linux/` when this
  lands to match `macos/` and house `global_shortcut.rs` there.
- `docs/control-api.md` — socket used by the current workaround.
- ashpd docs: <https://docs.rs/ashpd/0.13.10/ashpd/desktop/global_shortcuts/>
- Portal spec: <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html>
