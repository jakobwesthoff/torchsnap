---
kind: feature
status: needs-discussion
area: [src-tauri/src/frecency/mod.rs, src/settings/sections/FrecencySection.tsx]
tags: [ux, privacy]
---

# Usage statistics in the settings

The user, 2026-10-04: we might want a statistics store and a settings
tab that shows how often the launcher was called, how often per day,
what was executed and similar numbers. An idea worth looking at,
nothing decided yet.

## Today

- The frecency store records one row per executed item:
  `frecency_events (gadget_id, item_id, timestamp)`
  (`src-tauri/src/frecency/schema.rs`). `FrecencyStore::stats`
  (`src-tauri/src/frecency/mod.rs:327`) already returns totals, unique
  items, events per gadget and the oldest event, and
  `src/settings/sections/FrecencySection.tsx` shows them.
- Frecency events are deleted after 90 days (`MAX_AGE_MS`,
  `src-tauri/src/frecency/mod.rs:46`) and only exist while frecency is
  enabled. Statistics over a longer time, or with frecency switched off,
  need storage of their own.
- Nothing records that the launcher window was opened. Calls per day
  need a new event, written wherever the hotkey or tray shows the
  panel.

## Open questions

- Which numbers: calls per day, executions per gadget, top items, time
  of day, mascots seen?
- Raw events or daily aggregates? Aggregates keep the store small and
  hold less detail about what was searched.
- Retention, an off switch, and a button that clears the data, like
  the one frecency has.
- New tab, or an extension of the frecency section?

The data stays on the machine like everything else Torchsnap stores,
since the project promises no telemetry.
