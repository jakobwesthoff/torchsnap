# "Open settings" on a gadget entry opens nothing

**Kind:** bug
**Status:** found 2026-09-21, not fixed

## Symptom

Activating an entry whose action is `ActionId::OpenSettings` closes the
launcher and nothing else happens. No settings window appears. Seen with
the ZeroTier gadget's "ZeroTier token not configured" entry, whose footer
shows "Open settings", in a release build on macOS (the same code is on
`main`, so it is not caused by the dependency updates it was found
during).

## Cause

`GadgetHost::execute` intercepts `OpenSettings` before dispatching to the
gadget (`src-tauri/src/gadget_host.rs:959-972`). It emits the Tauri event
`open-gadget-settings` with `{ "gadgetId": source }` and returns
`PostAction::Dismiss`. Nothing in `src/` listens for that event, so the
dismiss is the only effect.

## Pieces that already exist

- `crate::show_settings_window(app)` (`src-tauri/src/lib.rs:279`) opens
  the settings window. `PostAction::ShowSettings` uses it
  (`gadget_host.rs:992-993`), as do the tray menu's "Settings..." entry
  and the built-in `settings` command (`src-tauri/src/gadgets/commands.rs:73`).
- The settings window keeps its selected section in local React state
  (`src/settings/SettingsPanel.tsx`, `SettingsSidebar.tsx`); there is no
  way to tell it from outside which section to show.

## Fix outline

1. Open the window: call `show_settings_window(app)` in the
   `OpenSettings` branch instead of only emitting the event.
2. Select the gadget's section: give the settings window a way to receive
   a target section, for example an event the settings webview listens
   for, or state the host sets before showing the window, so it opens on
   the gadget's page rather than General.
3. Decide what happens for a gadget without a settings section (open on
   General, or on the Gadgets management page).

## Related

`todos/gadget-host/api/01krp5a92mezsrq8jg14mm3nqx-opensettings-passthrough.md` proposes
moving `OpenSettings` from a host intercept to a `PostAction` the gadget
returns. Whichever lands first should use the same "open settings at
section X" mechanism.
