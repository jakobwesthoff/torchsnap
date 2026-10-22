---
title: "Land the parked frontend debounce of gadget setting writes"
kind: bug
component: gadget-host
status: blocked
origin: review
tags: [api-design, concurrency]
---
# Land the parked frontend debounce of gadget setting writes

The published docs promise that rapid writes to one setting reach a gadget's `on_setting_changed` as one call with the final value, but every write is delivered, so a slider drag calls the gadget once per step. A fix that debounces the writes in the frontend is implemented and reviewed, but parked on 2026-10-22 on local branches, unpushed and unmerged, until it has been tried in a build. Pick it up from those branches; nothing needs to be rewritten.

## Where the parked work is

- torchsnap, local branch `parked/debounce-setting-writes`: commit 48c67783 (records the decision in this todo) and 4d41c1b8 "Debounce gadget setting writes in the frontend" on top of 19fa0751. 4d41c1b8 deletes this todo; when landing, rebase onto main and drop the todo edit from 48c67783, since this note supersedes it.
- torchsnap-docs, local branch `parked/debounce-setting-writes`: commit f45b7d4 "Describe how quickly repeated setting changes reach a gadget" on top of 6f88e36.
- Plan and review ledger of the run (session scratchpad, may be gone): `setting-debounce-plan.md` and `setting-debounce-ledger.md`; 25 review findings, 19 fixed, 1 withdrawn, 3 duplicates, 2 decided.

## Problem

The claim "the host deduplicates rapid writes" appears in torchsnap-docs `development/interfaces/exports.mdx` (Aside on `on_setting_changed`) and `development/frontend/settings-panels.mdx`, in `docs/api/gadget-development.md`, in ADR 26 and ADR 29, in the Gadget-Architecture pages 01, 04 and 06, in `docs/strategy/Selfcontained-Gadget-System.md` §5.6, in the WIT comment, the template gadget and a `src-tauri/src/wasm/bridge.rs` comment, and as the "Good" example in torchsnap-docs' writing guide. Each settings-changed listener call delivers only its own change, so nothing is coalesced. No other source writes gadget settings today: the Control API has no settings method, nothing watches `settings.json`, and only `useGadgetSetting` writes `gadgets.<id>.*`.

## Decisions

- Jakob, 2026-10-21: debounce in the frontend (variant 2), not in the host (variant 1) and not only by correcting the docs (variant 3). Reason as accepted: it fixes the case authors were told about, rapid UI changes such as slider drags, for a fraction of the cost of host-side debouncing, which needs per-slot timers and ordering against switch changes.
- Lead, 2026-10-21: the docs state the true limit instead of naming other sources: changes in quick succession (about 150 ms apart) reach the gadget once with the final value; a value that holds longer reaches it each time, so expensive per-change work must tolerate repeated calls.
- Lead: the new ADR (0068 on the branch) amends ADR 26 and 29; every gadget setting change reaches the store, other windows and the gadget about 150 ms later, which the ADR states.
- Lead: the settings window flushes pending writes on a close request (bounded to 1 s) and then destroys itself; `core:window:allow-destroy` is granted only to the settings window, in its own capability file `src-tauri/capabilities/settings.json`.

## What the branch does

- `src/settingsStore.ts`: `setSettingDebounced`, `flushSetting`, `flushSettingWrites`, a 150 ms trailing debounce per key with one queue per key; the value shows at once, an unchanged value is not written, a failed write re-reads the store, per-call tokens keep a write from clearing a newer value. Pending writes are flushed on `pagehide`, when the document becomes hidden, and before a direct write of the same key.
- `src/contexts/useGadgetSetting.ts`: writes through the debounce, flushes its key on unmount or key change.
- `src/settings/flushSettingWritesOnClose.ts` (called from `src/settings/main.tsx`): on a close request waits for the writes (at most 1 s), removes its own listener, then lets Tauri destroy the window; a second close request during the wait is ignored. The listener must remove itself because Tauri keeps a closed page's listener ids, and a leftover id would make the next settings window impossible to close.
- 27 tests with fake timers (`act` plus `advanceTimersByTimeAsync`), repeated 20 times green; each of 13 parts of the fix was removed once and its test failed. `just fullcycle` was green on the branch.

## What blocks landing

- Not tried in the running app: change a gadget setting and close the settings window at once, by its title-bar button and by Cmd+W; reopen it and close it again (the second close shows no leftover listener blocks it). Only the JavaScript side is tested; the Tauri behaviour was read in its source (tauri 2.11.6, `manager/window.rs`).
- Unverified in WKWebView: whether `visibilitychange` fires when a Tauri window hides; the flush on it is best-effort.

## Known limits (recorded in ADR 0068 on the branch)

- A change made less than 150 ms before the app quits is lost.
- While the settings page's script hangs, its close control and Cmd+W do nothing.
- A reloaded settings page whose start fails before it registers the close handler cannot be closed (practically only with the dev server).
- Reopening settings during the up to 1 s close wait closes the window again.

## Relations

- Blocked by: a manual close test of the settings window in a build, as described above
