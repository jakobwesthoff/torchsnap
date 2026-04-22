# Linux tray: no left/right click differentiation

`src-tauri/src/platform/fallback/tray.rs` is built around the macOS
convention: left-click toggles the launcher, right-click opens the
menu. The `TrayIconBuilder` is configured with:

- `.show_menu_on_left_click(false)` — menu only on right-click
- `on_tray_icon_event` filter for `MouseButton::Left` +
  `MouseButtonState::Up` — triggers `on_toggle`

On Fedora/GNOME (and typically any Linux running via
`libayatana-appindicator3` through `tray-icon` / `libappindicator-sys`)
this does not work: clicks come through the AppIndicator /
StatusNotifierItem D-Bus protocol, which historically does not
forward raw mouse-button events to the app. Canonical deliberately
stripped the activation signal from `libappindicator`, and the
freedesktop SNI spec's `Activate` / `SecondaryActivate` events are
handled by the host (the GNOME extension, KDE Plasma, Waybar, …),
not uniformly. In practice today on GNOME the behaviour is: every
click opens the menu, there is no way to trigger the launcher from
the tray.

So the launcher is currently unreachable from the tray on Linux.

## Fix

Add an explicit **Open Launcher** entry at the top of the menu and
wire it to the same `on_toggle` callback that the left-click handler
uses. On macOS/Windows the left-click path keeps working; on Linux
the menu item is the only reliable activation path but costs nothing
on the other platforms.

Sketch (see `tray.rs`):

```rust
let open_item = MenuItem::with_id(app, "open", "Open Launcher", true, None::<&str>)
    .context("create Open Launcher menu item")?;
// ...
let menu = Menu::with_items(app, &[&open_item, &separator_top, &settings_item, &devtools_item, &separator, &quit_item])?;
// ...
.on_menu_event(move |app, event| match event.id.as_ref() {
    "open" => on_toggle(app),
    // ...
})
```

Consider whether to also surface the current launcher global-shortcut
binding in the menu item label (e.g. "Open Launcher   ⌥⇧Space") once
that wiring exists elsewhere — nice-to-have, not blocking.

## Worth also looking into

- Does the `tray-icon` crate expose `TrayIconEvent::Click` at all on
  Linux, or does the Linux backend only emit menu events? If clicks
  *do* fire via KSNI hosts that implement `SecondaryActivate`, the
  left-click path could be kept functional on those DEs — but the
  menu item is the portable fallback regardless.
- Evaluate whether `show_menu_on_left_click(true)` should be set
  under `#[cfg(target_os = "linux")]` so the existing single-click
  behaviour of "open the menu" stays consistent with other GNOME
  tray citizens rather than being a left-click-does-nothing corner
  case.

## Pointers

- `src-tauri/src/platform/fallback/tray.rs` — whole file
- The inline `TODO` at the top of that file already flags
  "Linux: tray protocol varies (StatusNotifierItem vs. XEmbed)"
