---
kind: bug
severity: medium
status: open
area: [src-tauri/src/wasm/runtime/instance.rs]
tags: [privacy, unconfirmed]
---

# `search` span records the full user query as log metadata (per-keystroke, cross-gadget-readable via the release devtools console)

## Problem
`WasmGadgetInstance::search` records the full user query as span
metadata (`instance.rs:264`):

```rust
let span = self.logger.span("search").meta("query", query).start();
```

`query` is the per-keystroke launcher input. The adjacent `execute`
span records `entry_id` (`instance.rs:304`), which for the app-launcher
gadget is a filesystem path.

## Impact

### Log-pipeline exposure (in-memory, but cross-gadget-readable)
Storage is in-memory only: span items flow through a bounded mpsc into
a logging task (`logging/channel.rs:238-260`) that pushes into
`RingBufferStorage`, a `VecDeque` capped at 10,000 items
(`logging/mod.rs:55`, `storage.rs:65-105`). No disk persistence
anywhere (no `tauri-plugin-log`, no file writes, verified); the buffer
is discarded on exit and `devtools_log_clear` empties it on demand. The
query enters the stream **twice per search call** (the `SpanStart` item
carries the metadata immediately, `spans.rs:341-367`, and the `SpanEnd`
item merges start+end metadata, `spans.rs:145-166`).

Readers of the buffer:
- Tauri commands `devtools_log_history` / `_subscribe` / `_clear` /
  `_stats` (`logging/commands.rs:69-164`, registered `lib.rs:602-605`).
- **The devtools window is a production feature, not debug-gated** —
  the tray shows "Developer Tools…" unconditionally
  (`platform/macos/tray.rs:31-33`, `platform/fallback/tray.rs:35-36`),
  and `show_devtools_window` is a registered command.
- No export-to-file (only per-item copy-to-clipboard,
  `src/devtools/console/LogItemRow.tsx:84-87`).
- WASM gadgets cannot read logs (the WIT `logging` interface is
  write-only: `log`/`span-start`/`span-end`).
- **But gadget frontend UI bundles can.** Gadget JS bundles are
  `import()`ed into the main webview (`wasmPluginLoader.ts:54-92`); the
  `devtools_log_*` commands take no window handle and do no window-label
  check, and there are no app-command permission definitions
  (`capabilities/default.json` grants `["main","settings","devtools"]`).
  So JS from any installed gadget's UI bundle can invoke
  `devtools_log_history` and read the entire ring buffer, including
  search spans and log items belonging to *other* gadgets and the host.

Calibration caveat: gadget UI bundles already run unsandboxed in the
main webview and could observe the live search input directly, so the
log read is incremental — but it grants **historical** data from before
the gadget's UI ever mounted, which live DOM access does not. (The
main-webview gadget-JS trust boundary is a separate, larger issue,
shared with the CSP/CORS findings.)

Net: in-memory, bounded, non-persisted — but readable by the production
devtools console and by any gadget UI bundle, cross-gadget.

### What is recorded, and sensitivity
- **Per-keystroke, fanned out.** `useSearch.ts` fires the `search`
  command on every query change with no debounce (`useSearch.ts:153,158`);
  in non-prefix mode the host dispatches the full query to *every*
  active WASM gadget (`gadget_host.rs:767-777`), each recording it. So
  one keystroke produces 2 × (active gadget count) log items containing
  the query fragment; the 10k buffer holds a deep recent history of
  everything typed.
- **Elided searches still record.** The span (with query) is created
  before the generation check (`instance.rs:264` vs `:267`), so even
  superseded stale calls capture their fragment.
- **Prefix mode records exactly the sensitive payload.** In prefix mode
  the host strips the prefix and passes the remainder
  (`gadget_host.rs:675,682`), so the span records precisely what the
  user typed after the prefix — for a hypothetical secrets/password/URL
  gadget, the highest-sensitivity input in the app. Launcher queries
  generally contain file names, contact names, and pasted content
  (URLs with embedded tokens are a realistic paste).
- **`execute` `entry_id`** is gadget-defined; for the app launcher a
  filesystem path. User-action history, lower volume (one per explicit
  action, not per keystroke) and lower sensitivity than free text, but a
  file-search gadget would put full home-directory paths here.
- The `search` query is the outlier in a file that otherwise shows
  selective-metadata discipline: `entries` records only `result_count`,
  `handle_message` records `method` not `payload`, `on_setting_changed`
  records `key` not `value`. The host span is the only recorder of the
  query (verified).

### Severity: low–medium
Low on the persistence axis (in-memory, bounded, gone at exit, no
export). What lifts it above pure-low: per-keystroke capture volume,
verbatim capture of post-prefix payloads, the devtools console shipping
in release builds, and cross-gadget readability of the buffer by any
gadget UI bundle. First-party-only ecosystem → low; the moment
third-party gadgets exist → a solid medium.

## Suggested fix — redact at record time, not display time
Display/export-time redaction is the wrong layer: storage *is* the
exposure (`devtools_log_history` returns raw `LogItem`s to whatever JS
asks), so the raw query must never enter the mpsc channel.

1. **Default: replace `.meta("query", query)` with
   `.meta("query_len", query.len().to_string())`** at `instance.rs:264`.
   The span's debugging value (duration, `response_type`,
   `result_count`, `elided`) is preserved, and length keeps the
   perf-relevant signal (long vs short input). Do not hash — launcher
   fragments are low-entropy and trivially brute-forceable, so a hash
   adds no privacy and no debug value.
2. **Opt-in verbose capture:** a single explicit debug setting (a
   devtools "capture query text" toggle, default off, ideally
   session-scoped rather than persisted so it cannot be left on
   accidentally) checked at the span call site; when on, record the raw
   query as today. Keeps the "why did this query return the wrong
   results" workflow available deliberately.
3. **Gate `execute`'s `entry_id`** (`instance.rs:304`) behind the same
   flag — default recording nothing or the source-gadget id only. Lower
   urgency (user-initiated, low volume), but one flag for both is the
   simplest mental model.
4. **Convention note for gadget authors:** host-side redaction cannot
   stop a gadget logging its received query via the WIT `logging`
   import. The achievable guarantee is "the host never records query
   text by default"; bundled gadgets should follow the same convention
   (belongs in the gadget SDK docs; unenforceable for third-party code).

## Adjacent (surfaced here, not B9 itself)
The `devtools_log_*` commands being invokable from the main webview
means gadget UI bundles read the full cross-gadget log stream. If/when
the gadget-UI trust boundary is tightened, restrict `devtools_log_*` to
the `devtools` window label (check the window in the command, or use
Tauri app-command permissions) — a cheap hardening step. Shares the
architectural root with the CSP/asset and CORS findings.

## Key files
`instance.rs:264,304`; `wasm/logging/{spans.rs,channel.rs,storage.rs,commands.rs,mod.rs}`;
`gadget_host.rs:660-828`; `src/launcher/hooks/useSearch.ts:153`;
`src/gadgets/wasmPluginLoader.ts:54-92`; `capabilities/default.json`.
