---
kind: investigation
status: deferred
area: [gadgets/awake]
tags: [api-design, wasm, macos]
---

# Awake: evaluate automatic safeguards for keep-awake sessions

The native keep-awake (`docs/research/native-keep-awake.md`) ships with
no safeguards: a session runs until the user stops it, its timer ends,
Torchsnap quits or the gadget is disabled. The user decided this on
2026-10-03 and asked for this todo to evaluate the candidates below as
later extensions of the gadget, including new host interfaces such as a
battery one.

## Candidates

- **Pause while the screen is locked.** Vorssaint listens for the
  `com.apple.screenIsLocked` and `com.apple.screenIsUnlocked`
  distributed notifications, releases its assertions and stops its
  jiggle while locked, and on unlock re-applies them or ends the session
  if its timer ran out meanwhile.
- **Battery limit.** Vorssaint ends a session on battery power at or
  below a limit (0, 5, 10, 15 or 20 %, default 10), checked every 30 s,
  and does not start automatic sessions below it.
- **Stop in Low Power Mode.** KeepingYouAwake has a
  `KYALowPowerModeMonitor` package next to its `KYABatteryMonitor` for
  deactivating on these conditions.

The research doc's section 6 lists the wider set of automatic start and
stop conditions as compartment C10 (external display, running apps,
power source, fullscreen, media). They belong to the same evaluation.

## Constraints from the gadget runtime

- A WASM gadget only runs when the host calls one of its exports and
  cannot receive pushed events (ADR 0030). It cannot react to a screen
  lock or a battery change by itself.
- Its finest periodic hook is a one-minute cron task (ADR 0032), which
  could poll a host query such as battery level or power source.
- The alternative is host-side policy: the gadget passes conditions
  when it acquires the wake lock, and the host's power service watches
  them and releases the lock.

## Questions to answer

- Which safeguards are worth having, and which default on.
- Gadget polling through new query interfaces (`battery`, power source,
  screen lock state) versus conditions enforced by the power service.
- How a session ended by a safeguard is reported, which ties into
  `todos/gadget-host/api/01m414x1hq141vaaec7aj0y7x6-notification-host-api.md`.
