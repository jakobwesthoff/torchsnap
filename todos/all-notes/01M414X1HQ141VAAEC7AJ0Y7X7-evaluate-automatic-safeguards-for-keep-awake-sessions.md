---
title: "Evaluate automatic safeguards for keep-awake sessions"
kind: investigation
component: gadgets
status: needs-discussion
horizon: someday
origin: discussion
tags: [api-design, macos]
---

# Evaluate automatic safeguards for keep-awake sessions

The native keep-awake (`docs/research/native-keep-awake.md`) ships with
no safeguards: a session runs until the user stops it, its timer ends,
Torchsnap quits or the gadget is disabled. This was raised in the
discussion that settled the design on 2026-10-03, and the user asked for
this todo to evaluate the candidates below as later extensions of the
gadget, including new host interfaces such as a battery one.

## Decisions

- 2026-10-03, user: native keep-awake ships with no safeguards. The
  candidates below are later extensions to evaluate.

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
- How a session ended by a safeguard is reported. This ties into the
  notification host API todo, see Relations.

## Revisit when

No trigger known; reconsider at the next sweep.

## Relations

- Relates to: [Add a notification host API for WASM gadgets](01M414X1HQ141VAAEC7AJ0Y7X6-add-a-notification-host-api-for-wasm-gadgets.md), for how a session ended by a safeguard is reported
