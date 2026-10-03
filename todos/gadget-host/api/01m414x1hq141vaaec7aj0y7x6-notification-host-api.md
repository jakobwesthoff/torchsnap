---
kind: feature
status: deferred
area: [gadgets/gadget-sdk/wit/torchsnap-gadget.wit, src-tauri/Cargo.toml]
tags: [api-design, wasm, macos]
---

# Notification host API for WASM gadgets

A WIT import that lets a WASM gadget post a desktop notification. Parked
on purpose: the user decided on 2026-10-03 that the native keep-awake
work (`docs/research/native-keep-awake.md`, section 9) ships without an
end-of-session notice, and that this API is evaluated separately.

## Use case

The awake gadget starts timed keep-awake sessions ("awake 30m"). Once
Torchsnap owns the timer, a session can end while the user is away from
the launcher, and nothing tells them it ended. Vorssaint, one of the
keep-awake tools the research looked at, posts a notification when a
session ends by timer or by its battery limit, and none on a manual
stop or quit.

## What the gadget runtime offers today

- The world imports `logging`, `clipboard`, `sql-storage`, `frecency`,
  `settings`, `opener`, `http`, `filesystem`, `assets`, `command`,
  `platform`, `path-resolver`, `types` and `website-metadata`, and no
  notification interface (`torchsnap-gadget.wit:936-949`).
- `src-tauri/Cargo.toml` has no notification plugin.
  `tauri-plugin-notification` exists on crates.io.
- Gadget code runs only when the host calls one of its exports:
  `lifecycle`, `search`, `messaging` or `tasks`
  (`torchsnap-gadget.wit:951-954`). WASM gadgets cannot stream to the
  frontend (ADR 0030).

## The timing problem

Discussed with the user on 2026-10-03. A `notification` import by itself
is no problem, since a gadget calling a host function is not a push in
the sense of ADR 0030. The difficulty for the awake use case is when to
call it: the session deadline fires while none of the gadget's exports
is running, so the gadget has no code running at that moment to call
`notify`.

Three ways were discussed:

- **(a) Cron poll.** The gadget declares a `[[tasks]]` entry that runs
  every minute, checks the lock's `is-held`, and calls `notify` when a
  session it started has ended. Works with the existing task system.
  Cron schedules are 5-field POSIX, so the finest interval is one minute
  (ADR 0032) and the notice can arrive up to 60 s late. Fires due while
  the Mac sleeps are skipped (todo
  `todos/gadget-host/wasm/01kwfz4kkaq7spwnm2ncket1gk-scheduler-missed-fires.md`),
  but the next fire after wake still finds the ended session.
- **(b) Host-side notice.** The gadget passes the notice text when it
  acquires the wake lock, and the host's power service posts it when the
  deadline fires. On time, but it couples the power service to
  notifications.
- **(c) Expiry export.** A new guest export such as `on-lock-expired`.
  ADR 0032 records that adding a guest export breaks the WIT for every
  existing gadget, which must then implement it.

Preferred route: a general `notification` import plus (a). Not decided
beyond that.

## Open points

- Permission: a manifest flag gated like the other interfaces, and the
  macOS notification authorization prompt for Torchsnap itself. Vorssaint
  lists the Notifications permission for its end-of-timer note.
- Shape of the call: title and body at least; whether a gadget icon,
  sound or click action belongs in it.
- A new WIT interface needs an ADR (ADR 0036).

Related: `todos/gadget-host/wasm/01knwq80dhks7hbvdnmq5f8e0h-gadget-enable-result-return.md`
asks whether a failed gadget enable should surface a toast or
notification. That is host UI, not a gadget API, but would share the
host side.
