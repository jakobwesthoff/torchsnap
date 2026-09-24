---
kind: bug
status: open
tags: [macos]
---

# macOS 27 tray left-click opens the menu instead of the launcher

Researched 2026-09-21, not fixed. Decision on approach still open
(recommendation below).

## Symptom

On macOS 27 (Golden Gate), every click on the menu bar icon opens the
context menu. Left-click no longer toggles the launcher, and
`show_menu_on_left_click(false)` has no effect. The launcher is still
reachable through the global hotkey and the "Open Launcher" menu entry
(e5c8e87).

Windows and Linux use `FallbackTray` and are not affected. Only
`src-tauri/src/platform/macos/tray.rs` is involved.

## Cause

Apple did not document a menu bar change. What the upstream bug
reports and fixes show: while an `NSMenu` is attached to an
`NSStatusItem`, AppKit on macOS 27 opens that menu itself on any mouse
button, and the status item's view never receives `mouseDown:` /
`mouseUp:`. Several reports tie this to AppKit moving input handling
onto gesture recognizers in 27. tray-icon #355 shows that removing the
attached menu brings back all four down/up events.

Related break reported by Wails (wails #6147): the deprecated
`NSStatusItem.setTarget` / `setAction` no longer fire on 27. Wails fixed
it by setting target/action on `statusItem.button`.

Our versions (from `src-tauri/Cargo.lock`): tauri 2.11.5, tray-icon
0.24.2, muda 0.19.3. tray-icon 0.24 attaches the menu to the status
item permanently at build time (`TrayIconBuilder::menu`) and in
`set_menu`. The `menu_on_left_click` flag is only read inside
tray-icon's own click handler (`on_tray_click`), which never runs on 27.

## Upstream status

- tray-icon: fixed in 0.25.1 (2026-09-16) by PR #365. The menu is
  attached only while it is being shown: `setMenu(menu)`,
  `performClick`, `setMenu(None)`, in both `on_tray_click` and
  `show_menu`. It also retains the menu out of the `RefCell` before
  `performClick`, so a `set_menu` while the menu is open does not
  panic. The diff is about 30 lines in
  `src/platform_impl/macos/mod.rs`. Tested by the author on macOS 27
  and 26.6.2.
- Tauri: the `dev` branch still depends on `tray-icon = "0.24"`.
  0.25 is semver-incompatible with 0.24 and also moves to muda 0.20,
  so `cargo update` cannot pick it up. It needs a Tauri release.
  tauri #16035 was closed with "will be available in the next
  release". No open Tauri PR for the bump as of 2026-09-21.

## Options

### A. Patch tray-icon (recommended)

Fork tray-icon at tag `tray-icon-v0.24.2`, apply the #365 diff, and
point `[patch.crates-io]` in `src-tauri/Cargo.toml` at the fork. The
fork must keep a 0.24.x version, or Cargo ignores the patch because it
doesn't satisfy Tauri's `0.24` requirement.

- No app code changes. Uses the fix the maintainers reviewed.
- One commenter on #365 reports running it as a patch successfully.
- Cost: a git dependency to carry until Tauri bumps tray-icon.
- Removal: delete the `[patch]` block once Tauri ships tray-icon 0.25.

### B. Work around it in `MacosTray`

Approach used by codex-pacer PR #38 and omniroute-tray PR #55:

1. Build the tray without `.menu()`, so click events arrive again.
2. Keep `TrayIconBuilder::on_menu_event`. Tauri dispatches it through
   `manager.menu.global_event_listeners` (`tauri-2.11.5/src/app.rs:2588`),
   so menu items still fire with no menu attached.
3. On right-button `Up`: `tray.set_menu(Some(menu))`, then
   `tray.with_inner_tray_icon(|t| t.show_menu())`, then
   `tray.set_menu(None::<Menu<_>>)`. This mirrors the upstream fix.

Pitfalls found while researching:

- Tauri calls the per-tray event listener while holding
  `manager.tray.event_listeners` (`app.rs:2616`). `show_menu` runs a
  nested event loop until the menu closes. A tray event dispatched
  during that loop would lock the same `std::sync::Mutex` on the same
  thread and deadlock. The step-3 sequence therefore has to run after
  the listener returns.
- `AppHandle::run_on_main_thread` does not defer when called on the
  main thread. `send_user_message` runs the closure immediately in that
  case (`tauri-runtime-wry/src/lib.rs:239`). Deferring needs a post
  from another thread (so it goes through the event loop proxy) or GCD
  `dispatch_async` on the main queue.
- omniroute-tray #55 claims the inline call is safe, but they only
  tested on macOS 26 with a forced detached-menu mode.
- tray-icon highlights the button on any mouse-down and clears it only
  in the left `mouseUp:` handler. omniroute had to reset the highlight
  by hand after right-clicks because they present the menu at the
  pointer without attaching it. With the attach-then-`performClick`
  path, AppKit's menu tracking should handle the highlight, but that
  needs checking on 27.
- Tauri's docs warn that `with_inner_tray_icon` ties the code to a
  specific tray-icon minor version.

B puts main-thread re-entrancy logic into our code for a problem that
disappears with the next Tauri release. A is less code and easier to
remove.

## When done

- Verify on macOS 27: left-click toggles the launcher, right-click and
  ctrl-click open the menu, menu items work, icon highlight clears.
- Verify on macOS 26 if a machine is available.
- Add a CHANGELOG "Unreleased" entry (user-visible bug).
- Revisit the "Open Launcher" menu entry comment in `tray.rs` only if
  behavior changes. It stays useful either way.

## Sources

- https://github.com/tauri-apps/tray-icon/issues/355
- https://github.com/tauri-apps/tray-icon/pull/365
- https://github.com/tauri-apps/tray-icon/releases/tag/tray-icon-v0.25.1
- https://github.com/tauri-apps/tauri/issues/16035
- https://github.com/wailsapp/wails/issues/6147
- https://github.com/RyanZhangNTU/codex-pacer/pull/38
- https://github.com/zoispag/omniroute-tray/issues/54
- https://github.com/zoispag/omniroute-tray/pull/55
- https://mjtsai.com/blog/2026/06/18/appkit-in-macos-27/
