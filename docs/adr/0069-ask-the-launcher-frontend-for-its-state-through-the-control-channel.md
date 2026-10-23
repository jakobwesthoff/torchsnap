# 69. Ask the launcher frontend for its state through the control channel

Date: 2026-10-23

## Status

Accepted

## Context

The control API answers from the backend: `status` reports what the
backend knows about the launcher window. The query, the result list, the
selection, the gadget views and whether the content is blanked live only
in the launcher's React state. A test that drives Torchsnap through the
control socket needs them; without them it compares screenshots.

The backend already sends commands to the launcher frontend over a
Tauri channel (`ControlCommand`, `useControlChannel`). Nothing came back
over it.

## Decision

The control method `snapshot` asks the launcher frontend for its state
and answers with the frontend's reply.

- The backend sends `ReportSnapshot { request_id }` over the existing
  control channel. The frontend answers with the Tauri command
  `control_report_snapshot(request_id, snapshot)`.
- The waiting requests live in a pending map in `ControlChannelState`,
  next to the channel they were sent on, each with that channel's id.
- A request waits at most a fixed time, two seconds. A caller sees `-4`
  when the frontend does not answer in time, `-1` at once when no
  frontend is connected, and `-3` at once when the answer is malformed.
- When a new page subscribes, the requests sent on the old channel are
  dropped and their callers get `-4` at once.
- The frontend builds the answer through a `getSnapshot` callback that
  the `Launcher` hands to `useControlChannel`, from a pure
  `buildLauncherSnapshot`. The hook knows the protocol; the `Launcher`
  knows its state.
- Control handlers run on Tauri's blocking pool (`spawn_blocking` in
  `handle_connection`), so a handler may block on the main thread or on
  the frontend's answer. The `Handler` trait stays synchronous.

## Consequences

- The app has one more Tauri command, `control_report_snapshot`.
- A `snapshot` answer comes from the launcher's last render, except
  `contentVisible` and `inputFocused`, which the frontend reads from the
  page when it handles the request.
- A waiting `snapshot` holds one blocking-pool thread for up to two
  seconds.
