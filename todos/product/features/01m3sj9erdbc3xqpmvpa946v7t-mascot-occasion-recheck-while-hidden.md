---
kind: improvement
severity: low
status: open
area: [src/mascot/useMascotVariant.ts, src/mascot/conditions.ts, src-tauri/src/lib.rs]
tags: [macos]
---

# Re-check the mascot's occasions while the launcher is hidden

Designed during the review of the per-character selection (ADR 59) on
2026-09-30 and deferred by the user: the approach first needs
validating on macOS. Until then the current behaviour stays, as ADR 59
records.

## Problem

The next mascot is drawn when the launcher is dismissed (`tauri://blur`
in `src/mascot/useMascotVariant.ts`), so its image is already loaded when
the launcher shows again. The occasion conditions (full moon at night,
New Year's Eve, Christmas, Halloween, Easter) are evaluated at that
moment. If the launcher stays hidden across an occasion's edge (dusk on
a full-moon day, Dec 31 20:00, midnight into Dec 20), the first launch
after the edge shows a draw from the other side. With the shares of
ADR 59 (FullMoon and NewYear 90%) that is visible: the first launch of a
full-moon evening usually shows no full-moon costume.

## Designed fix (not validated)

- The hook tracks hidden and visible itself (hidden on `tauri://blur`,
  visible on `tauri://focus`).
- While hidden, a 60 s interval evaluates the set of condition names that
  hold now and compares it with `conditionsHeld` of the current draw
  record. Only when they differ does it draw again, from the same recent
  list and without recording anything in it, through the same pure state
  updater as the blur draw. Settings are read with `getSettingSync`.
- The new image then loads while the launcher is hidden. The interval
  runs only while hidden, so the mascot never changes in front of the
  user. It lives in the same effect as the blur listener, with cleanup
  on unmount.
- Tests with fake timers: no re-draw while the set is unchanged, exactly
  one when it changes, none while visible.
- Cost: a few date and moon calculations per minute; a re-draw only at
  an edge.

## What has to be validated first

- **Timers in the hidden launcher on macOS.** On dismiss the launcher
  panel is hidden and the window shrunk to 1x1
  (`src-tauri/src/lib.rs`, `hide_launcher` and `shrink_launcher_window`,
  ADR 20). Does WebKit still fire a `setInterval` there, and how late?
  App Nap or timer throttling may delay or suspend it.
- **Image loading while fully hidden.** Does a new `<img src>` rendered
  into the hidden 1x1 window actually load and decode before the next
  show? The blur pre-draw relies on this today (see the comment in
  `useMascotVariant.ts`); check whether it holds, and whether it still
  holds for a draw made minutes later.
- **Linux and Windows.** Linux hides `#root` and restores it after
  `launcher-shown` (`src/launcher/visibility.ts`); check that a re-draw
  while hidden does not interfere. Windows is untested.

## Alternatives considered

- **Re-check at show.** `launcher-shown` is emitted only on Linux
  (`show_launcher` in `src-tauri/src/lib.rs`, `#[cfg(target_os =
  "linux")]`). On macOS the first frontend signal, `tauri://focus`,
  comes after the panel is visible, so the mascot would swap in view and
  load an uncached image.
- **A backend event before showing, on every platform.** Needs a Rust
  change, and the show would have to wait for React and the image.
- **One timeout to the exact next edge.** The edges depend on sunset
  times and on how the moon distance is rounded; much more code for
  gaining less than a minute.
- **Nothing** (the current behaviour): accepted until this is validated.
