# Dev-Mode stdout Mirror for Gadget Logs

Mirror gadget/host `LogItem`s to stdout (or stderr) in debug builds so
`just start` output includes gadget logs.

**Current behavior:** `LogItem`s flow through `LogSender`
(`src-tauri/src/wasm/logging/channel.rs`) into in-memory storage
consumed by the frontend log UI. Nothing is written to stdout or to
disk, so in dev mode (`just start`) the process output contains only
Vite and cargo output.

**Why it matters:** during end-to-end verification of the `app-icon`
feature (2026-07-06), gadget-level warnings (e.g. the resolve pass
dropping an icon) were unobservable from outside the running app —
headless tooling had no way to see whether a gadget loaded, enabled,
or warned. The internal log UI is the only consumer.

**Status:** needs discussion

**Discussion needed:**
- `cfg(debug_assertions)`-only, or additionally gated behind an env
  var so dev runs stay quiet by default?
- Mirror point: inside `LogSender::send`, or as a second consumer on
  the channel next to the storage sink?
- Format: plain one-line rendering (timestamp, source, level,
  message) is enough for grep-ability; spans/metadata can stay
  UI-only.
