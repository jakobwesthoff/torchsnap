---
kind: feature
status: deferred
area: [src-tauri/src/platform/mod.rs, src-tauri/src/platform/macos/tray.rs]
tags: [ux, macos]
---

# Tray indicator while a wake lock is held

Show in the tray icon that Torchsnap is keeping the Mac awake. The user
liked the idea on 2026-10-03 but kept it out of the first native
keep-awake version (`docs/research/native-keep-awake.md`, section 9,
compartment C9).

## Today

- The tray is built once in `setup()` from the template image
  `src-tauri/icons/tray-icon-template.png`
  (`src-tauri/src/platform/macos/tray.rs:68-73`). Nothing changes the
  icon, tooltip or menu after that.
- `Tray::build` returns only the "Check for Updates..." menu item, which
  the updater relabels (`src-tauri/src/platform/mod.rs:136-153`). There
  is no handle for changing the icon.

## Discussed shape

The indicator follows the host's wake-lock table, not the awake gadget:
it is on while any gadget holds a lock through the `power` interface
proposed in the research doc (section 7.3), so a lock taken by another
gadget lights it up too.

Scope options discussed, none chosen:

- Icon only: a second template image, for example the torch with a dot,
  swapped in while the lock table is non-empty.
- Icon plus a tooltip or disabled menu line naming the holder and the
  end time.
- Icon, line and a "Stop keeping awake" menu item. The host then ends
  gadget-held locks on its own, which the awake gadget's status has to
  cope with.

Vorssaint, a reference tool in the research, has a menu bar icon with a
selectable symbol and tint and an optional countdown.

Depends on the `power` host service, which does not exist yet.
