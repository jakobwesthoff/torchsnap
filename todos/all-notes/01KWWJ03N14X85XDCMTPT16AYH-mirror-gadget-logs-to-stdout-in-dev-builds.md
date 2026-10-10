---
title: "Mirror gadget logs to stdout in dev builds"
kind: improvement
component: gadget-host
status: needs-discussion
---
# Mirror gadget logs to stdout in dev builds

Gadget and host `LogItem`s should also appear on stdout (or stderr) in debug builds, so `just start` output includes gadget logs.

## Current behavior
`LogItem`s flow through `LogSender`
(`src-tauri/src/wasm/logging/channel.rs`) into in-memory storage,
which the frontend log UI consumes. Nothing is written to stdout or to
disk. In dev mode (`just start`), the process output therefore contains
only Vite and cargo output.

## Why it matters
During end-to-end verification of the `app-icon` feature (2026-07-06),
gadget-level warnings, such as the resolve pass dropping an icon, were
unobservable from outside the running app. Headless tooling had no way to
see whether a gadget loaded, enabled, or warned. The internal log UI is
the only consumer.

## Open questions
- Should the mirror be `cfg(debug_assertions)`-only, or additionally gated
  behind an env var so dev runs stay quiet by default?
- Where is the mirror point: inside `LogSender::send`, or as a second
  consumer on the channel next to the storage sink?
- Format: a plain one-line rendering (timestamp, source, level, message) is
  enough for grep-ability. Spans and metadata can stay UI-only.
