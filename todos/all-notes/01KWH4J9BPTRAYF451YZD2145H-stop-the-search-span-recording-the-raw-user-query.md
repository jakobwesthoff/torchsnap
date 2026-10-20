---
title: "Stop the search span recording the raw user query"
kind: bug
component: gadget-host
tags: [privacy]
---
# Stop the search span recording the raw user query

The search span records the full launcher input as span metadata on every keystroke. The release devtools console can read that buffer, and any installed gadget UI bundle can read it too, so typed content such as file names, contact names or pasted tokens sits readable in the in-memory log. The fix is to record the query length by default and make raw capture an explicit opt-in.

## Problem
Read in the code, not reproduced.

`WasmGadgetInstance::search` records the full user query as span
metadata (`WasmGadgetInstance::search` in `instance.rs`):

```rust
let span = self.logger.span("search").meta("query", query).start();
```

`query` is the per-keystroke launcher input. The adjacent `execute`
span (`WasmGadgetInstance::execute` in `instance.rs`) records no metadata at all since ADR 0055
replaced the `entry_id`/`action_id` execute signature with a single
`command` string. The `entry_id` privacy concern therefore no longer
applies, and the `query` span is the sole remaining exposure this
todo tracks.

## Impact

### Log-pipeline exposure (in-memory, but cross-gadget-readable)
Storage is in-memory only. Span items flow through a bounded mpsc into
a logging task (`logging/channel.rs`) that pushes into
`RingBufferStorage`, a `VecDeque` capped at 10,000 items
(`logging/mod.rs:55`, `storage.rs:65-105`). No disk persistence
exists anywhere (no `tauri-plugin-log`, no file writes; verified).
The buffer is discarded on exit, and `devtools_log_clear` empties it on
demand. The query enters the stream **twice per search call**. The
`SpanStart` item carries the metadata immediately (`SpanBuilder::start` in `spans.rs`),
and the `SpanEnd` item merges start and end metadata (`SpanRegistry::end` in `spans.rs`).

Readers of the buffer:
- Tauri commands `devtools_log_history` / `_subscribe` / `_clear` /
  `_stats` (`logging/commands.rs:69-164`, registered `lib.rs:602-605`).
- **The devtools window is a production feature, not debug-gated.**
  The tray shows "Developer Tools…" unconditionally
  (`platform/macos/tray.rs:31-33`, `platform/fallback/tray.rs:35-36`),
  and `show_devtools_window` is a registered command.
- No export-to-file exists. Only per-item copy-to-clipboard does
  (`src/devtools/console/LogItemRow.tsx:84-87`).
- WASM gadgets cannot read logs. The WIT `logging` interface is
  write-only: `log`, `span-start`, `span-end`.
- **But gadget frontend UI bundles can.** Gadget JS bundles are
  `import()`ed into the main webview (`wasmPluginLoader.ts:54-92`). The
  `devtools_log_*` commands take no window handle and do no window-label
  check, and there are no app-command permission definitions
  (`capabilities/default.json` grants `["main","settings","devtools"]`).
  So JS from any installed gadget's UI bundle can invoke
  `devtools_log_history` and read the entire ring buffer, including
  search spans and log items belonging to *other* gadgets and the host.

Calibration caveat: gadget UI bundles already run unsandboxed in the
main webview and could observe the live search input directly, so the
log read is incremental. It does grant **historical** data from before
the gadget's UI ever mounted, which live DOM access does not. The
main-webview gadget-JS trust boundary is a separate, larger issue,
shared with the CSP and CORS findings.

Net: in-memory, bounded, non-persisted, but readable by the production
devtools console and by any gadget UI bundle, cross-gadget.

### What is recorded, and sensitivity
- **Per keystroke, fanned out.** `useSearch.ts` fires the `search`
  command on every query change with no debounce (`useSearch.ts:153,158`).
  In non-prefix mode the host dispatches the full query to *every*
  active WASM gadget (`GadgetHost::search` in `gadget_host.rs`), and each one records it.
  One keystroke therefore produces 2 × (active gadget count) log items
  containing the query fragment. The 10k buffer holds a deep recent
  history of everything typed.
- **Elided searches still record.** The span (with query) is created
  before the generation check (in `WasmGadgetInstance::search`), so even
  superseded stale calls capture their fragment.
- **Prefix mode records exactly the sensitive payload.** In prefix mode
  the host strips the prefix and passes the remainder
  (the prefix path of `GadgetHost::search`). The span therefore records precisely what the
  user typed after the prefix. For a hypothetical secrets, password or URL
  gadget, that is the highest-sensitivity input in the app. Launcher queries
  generally contain file names, contact names, and pasted content.
  URLs with embedded tokens are a realistic paste.
- The `search` query is the outlier in a file that otherwise shows
  selective-metadata discipline. `entries` records only `result_count`,
  `handle_message` records `method` not `payload`, and `on_setting_changed`
  records `key` not `value`. The host span is the only recorder of the
  query (verified).

### Severity: low to medium
Low on the persistence axis: in-memory, bounded, gone at exit, no
export. What lifts it above pure low is the per-keystroke capture volume,
verbatim capture of post-prefix payloads, the devtools console shipping
in release builds, and cross-gadget readability of the buffer by any
gadget UI bundle. With a first-party-only ecosystem it stays low. The
moment third-party gadgets exist, it becomes a solid medium.

## Suggested fix
Redact at record time, not display time. Display or export-time redaction
is the wrong layer. Storage is the exposure, because `devtools_log_history`
returns raw `LogItem`s to whatever JS asks, so the raw query must never
enter the mpsc channel.

1. **Default: replace `.meta("query", query)` with
   `.meta("query_len", query.len().to_string())`** in `WasmGadgetInstance::search`.
   The span's debugging value (duration, `response_type`,
   `result_count`, `elided`) is preserved. Length keeps the
   perf-relevant signal, long versus short input. Do not hash.
   Launcher fragments are low-entropy and trivially brute-forceable, so a hash
   adds no privacy and no debug value.
2. **Opt-in verbose capture:** a single explicit debug setting (a
   devtools "capture query text" toggle, default off, ideally
   session-scoped rather than persisted so it cannot be left on
   accidentally) checked at the span call site. When on, record the raw
   query as today. This keeps the "why did this query return the wrong
   results" workflow available deliberately.
3. **Convention note for gadget authors:** host-side redaction cannot
   stop a gadget logging its received query via the WIT `logging`
   import. The achievable guarantee is "the host never records query
   text by default". Bundled gadgets should follow the same convention.
   This belongs in the gadget SDK docs and is unenforceable for third-party code.

## Adjacent (surfaced here, not part of the fix above)
The `devtools_log_*` commands being invokable from the main webview
means gadget UI bundles read the full cross-gadget log stream. If the
gadget-UI trust boundary is tightened, restrict `devtools_log_*` to
the `devtools` window label (check the window in the command, or use
Tauri app-command permissions). That is a cheap hardening step. It shares
the architectural root with the CORS finding.

## Relations

- Relates to: [Replace Origin reflection on the gadget asset protocol with an allowlist](01KWH4J9BPTRAYF451YZD2145B-replace-origin-reflection-on-the-gadget-asset-protocol-with-an-allowlist.md), the same gadget UI trust boundary

## Key files
`WasmGadgetInstance::search` in `instance.rs`; `wasm/logging/{spans.rs,channel.rs,storage.rs,commands.rs,mod.rs}`;
`gadget_host.rs`, `GadgetHost::search`; `src/launcher/hooks/useSearch.ts:153`;
`src/gadgets/wasmPluginLoader.ts:54-92`; `capabilities/default.json`.
