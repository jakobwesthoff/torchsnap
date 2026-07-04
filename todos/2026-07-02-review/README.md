# Full-codebase review — 2026-07-02

Findings from a structured review of the entire repository, split
into independent review operations. Each finding is a single
self-sufficient todo file named `<ulid>-<short-description>.md`
(ULID via `mkulid -l`), grouped by area:

- `host-wasm/` — WASM runtime, host capability implementations,
  manifest/permission parsing (`src-tauri/src/wasm/`).
- `host-core/` — non-WASM host code (`src-tauri/src/` caps,
  storage, control, settings, network, icons, entry store,
  frecency, commands).
- `platform/` — `src-tauri/src/platform/` and built-in gadgets
  with platform integration (`src-tauri/src/gadgets/`).
- `gadgets/` — WASM gadget crates under `gadgets/` and the Rust
  gadget SDK + WIT.
- `frontend/` — `src/` React/TS code and `packages/gadget-sdk`.
- `build/` — Justfiles, Vite/Tauri/TS configs, `tools/`,
  packaging pipeline.
- `docs/` — documentation inaccuracies found while cross-checking
  code.

Each todo file follows this layout:

```markdown
# <Title>

**Kind:** bug | possible-bug | refactor | improvement | question
**Severity:** high | medium | low
**Area:** <primary file or module path>

## Problem
Self-sufficient description with exact `file:line` references and
quoted code. The reader must not need to rediscover the issue.

## Impact
What breaks or could break, and under which conditions.

## Suggested fix
Concrete direction if one is known; otherwise open questions.
```

Findings that duplicate an existing todo elsewhere under `todos/`
are not repeated here.

## Cross-boundary / verification pass (continuation session, 2026-07-02)

A second angle over the already-reviewed areas: run the
toolchain and audit the contracts *between* areas (Rust ↔ TS
mirrors, event vocabulary, sync-obligation comments) that the
per-area file review could not see as seams.

Checks that came back clean: `cargo test` (822/822),
`tsc --noEmit`, `eslint .`, MPL header sweep (only the
already-tracked `init-firewall.sh` missing), `CommandMap` ↔
`generate_handler!` (22/22), `PostAction` / `FrecencyStats` /
`ControlCommand` / `GadgetSourceKind` / `WasmGadgetManifest`
mirrors, gadget-SDK shim slices ↔ `initGadgetSdk` global,
`react-ready` handshake wiring, settings-key vocabulary (7/7
keys present on both sides), `settings-changed` emit discipline
(one gap, folded into the uninstall todo).

Findings produced by this pass:

- `build/01kwh2rsyn8ycx9teg40tpwy80-host-crate-53-clippy-warnings.md`
- `frontend/01kwh2rsyn8ycx9teg40tpwy81-actionid-mirror-missing-opensettings.md`
- `frontend/01kwh2rsyn8ycx9teg40tpwy82-sort-comparators-diverge-non-bmp.md`
- `host-core/01kwh386ce7p2p40xksx8gph3j-open-gadget-settings-event-has-no-listener.md`
- `frontend/01kwh386cftdb1194jvkk89p4g-toggle-theme-listener-dead.md`
- extended `frontend/01kwh13v4m52vpxqyj39x6rewg-command-ts-stale-module-reference.md`
  (compareEntries.ts + all stale types.ts section headers)
- escalated `frontend/01kwg3rq2j0y504d98nmmczfb0-matchescombo-case-sensitivity.md`
  to high (host default Reveal/OpenWith keybindings are dead on
  arrival)
- extended `host-core/01kwh2e8mne5bd05tpb4paacwc-uninstall-live-gadget-resurrects-state.md`
  (uninstall bypasses the settings-changed pipeline)

## Security pass (2026-07-02)

The deferred security pass ran as the final operation. Every
pointer that had been parked in `security-pass-queue.md` was
analyzed (each with an independent Fable advisor consult) and
written up as a self-contained todo; the queue is now a completion
index (`security-pass-queue.md`). This supersedes the "revisit in
the security pass" notes in the coverage gaps below for
`caps/http.rs` + `network/http.rs`, `caps/opener.rs`, the
`network/website_metadata/*` fetch/protocol, and the
`gadget_install.rs` install/uninstall trust chain — all now have
todos.

Headline results: the `command` capability is the sharpest area —
guest `cwd` unvalidated (**critical**), env-override injection
(`LD_PRELOAD`, **critical**/threat-model-gated), and non-absolute
binary PATH-hijack (**high**) together mean any command grant is
currently equivalent to arbitrary native code as the user. Other
**high** items: the unauthenticated compile-cache `deserialize_file`
(command-grant self-plant → host RCE), HTTP SSRF (redirects + DNS
rebind), and the null CSP + broad asset-protocol scope
(cross-gadget SQLite/settings read). One new finding surfaced
during the pass (`DirectorySource::open` reads `manifest.toml`
unguarded), and one queue item (app-launcher forged `entry.id`) was
downgraded from the initial framing to **low** by an entry-store
gate discovered during analysis.

`src-tauri/src/wasm/path_safety.rs` remains the sole area not
covered — excluded by direction; see the coverage gap below.

## Coverage gaps

- `src-tauri/src/wasm/path_safety.rs` was excluded from this
  review at Jakob's direction; no findings for it are recorded
  here and its review is still outstanding. This is now the only
  outstanding area after the security pass.
- `src-tauri/src/caps/http.rs` and
  `src-tauri/src/network/http.rs` were only partially reviewed
  (skipped ahead at Jakob's direction mid-analysis); the
  pointers gathered so far are parked in
  `security-pass-queue.md` and both files should be revisited
  in the security pass.
- `src-tauri/src/caps/opener.rs` was likewise only partially
  reviewed (skipped ahead); one pointer parked in
  `security-pass-queue.md`.
- `src-tauri/src/network/website_metadata/fetch.rs` and
  `.../website_metadata/protocol.rs` were likewise only
  partially reviewed (skipped ahead); pointers parked in
  `security-pass-queue.md`.
- `src-tauri/src/unicode.rs` — analysis completed in the
  continuation session (2026-07-02): no new findings; both
  observations (surrogate-pair splitting, out-of-range index
  passthrough) were already recorded in
  `host-core/01kwh1qj4vjt5q07vwntg4rpcb-highlight-splits-surrogate-pairs.md`.
- `src-tauri/src/commands/mod.rs` and `commands/types.rs` —
  analysis completed in the continuation session: no new
  findings beyond existing todos (the `expect`-on-JoinError
  panics are covered by
  `host-core/01kwg1ph0qcdqtara5jcw7abyk-gadget-panic-propagates-into-search.md`;
  webview-forged execute routing is parked in
  `security-pass-queue.md`).
- `src-tauri/src/gadget_install.rs` — general analysis
  completed in the continuation session: two findings recorded
  (`host-core/01kwh2e8mne5bd05tpb4paacwc-uninstall-live-gadget-resurrects-state.md`,
  `host-core/01kwh2e8mne5bd05tpb4paacwd-install-uninstall-blocked-until-restart.md`);
  the install/uninstall trust-chain pointer stays parked in
  `security-pass-queue.md` for the security pass.
